use std::{
   any::Any,
   collections::HashMap,
   error,
   fmt,
   panic::{
      AssertUnwindSafe,
      catch_unwind,
   },
   pin::Pin,
   sync::{
      Arc,
      Mutex,
      OnceLock,
      atomic::{
         AtomicBool,
         AtomicI64,
         Ordering,
      },
   },
   task::{
      Context,
      Poll,
   },
};

use jni::{
   JNIEnv,
   objects::{
      JObject,
      JString,
      JValue,
   },
   sys::{
      jlong,
      jstring,
   },
};
use pushcompat_listener::{
   AppRegistration,
   AppRegistrationState,
   DataMessage,
   DeviceSession,
   DeviceSessionState,
   FcmCredentials,
   Message,
   new_heartbeat_ack,
   new_stream_ack,
};
use serde::Deserialize;
use serde_json::{
   Value,
   json,
};
use tokio::{
   io::AsyncWriteExt,
   runtime::{
      Builder,
      Runtime,
   },
   sync::oneshot,
};
use tokio_stream::{
   Stream,
   StreamExt,
   StreamMap,
};

struct NativeHandle {
   http:           reqwest::Client,
   runtime:        Mutex<Runtime>,
   stop_requested: AtomicBool,
   stopper:        Mutex<Option<oneshot::Sender<()>>>,
}

impl NativeHandle {
   fn request_stop(&self) -> Result<()> {
      self.stop_requested.store(true, Ordering::Release);
      let stopper = self
         .stopper
         .lock()
         .map_err(|_| NativeError("native stop state is poisoned".into()))?
         .take();
      if let Some(stopper) = stopper {
         let _ = stopper.send(());
      }
      Ok(())
   }

   fn install_stopper(&self) -> Result<oneshot::Receiver<()>> {
      let (sender, receiver) = oneshot::channel();
      let mut stopper = self
         .stopper
         .lock()
         .map_err(|_| NativeError("native stop state is poisoned".into()))?;
      if stopper.is_some() {
         return Err(NativeError("native run is already active".into()));
      }
      *stopper = Some(sender);
      if self.stop_requested.load(Ordering::Acquire) {
         if let Some(sender) = stopper.take() {
            let _ = sender.send(());
         }
      }
      Ok(receiver)
   }
}

struct StopperGuard<'a>(&'a NativeHandle);

impl Drop for StopperGuard<'_> {
   fn drop(&mut self) {
      if let Ok(mut stopper) = self.0.stopper.lock() {
         stopper.take();
      }
   }
}

#[derive(Deserialize)]
struct RunRegistration {
   state:          AppRegistrationState,
   #[serde(default)]
   persistent_ids: Vec<String>,
}

struct NotifyClose<S> {
   stream: Option<S>,
}

impl<S> NotifyClose<S> {
   const fn new(stream: S) -> Self {
      Self {
         stream: Some(stream),
      }
   }

   fn stream_mut(&mut self) -> Result<&mut S> {
      self
         .stream
         .as_mut()
         .ok_or_else(|| NativeError("MCS stream is closed".into()))
   }
}

impl<S> Stream for NotifyClose<S>
where
   S: Stream + Unpin,
{
   type Item = Option<S::Item>;

   fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
      match self.stream.as_mut() {
         Some(stream) => {
            match Pin::new(stream).poll_next(context) {
               Poll::Ready(Some(item)) => Poll::Ready(Some(Some(item))),
               Poll::Ready(None) => {
                  self.stream = None;
                  Poll::Ready(Some(None))
               },
               Poll::Pending => Poll::Pending,
            }
         },
         None => Poll::Ready(None),
      }
   }
}

#[derive(Debug)]
struct NativeError(String);

impl NativeError {
   fn from_display(error: impl fmt::Display) -> Self {
      Self(error.to_string())
   }
}

impl fmt::Display for NativeError {
   fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
      formatter.write_str(&self.0)
   }
}

impl error::Error for NativeError {}

impl From<jni::errors::Error> for NativeError {
   fn from(error: jni::errors::Error) -> Self {
      Self::from_display(error)
   }
}

type Result<T> = std::result::Result<T, NativeError>;

const MAX_JNI_JSON_BYTES: usize = 1024 * 1024;
const MAX_MCS_IDENTITIES: usize = 128;
const MAX_PERSISTENT_IDS: usize = 500;

static HANDLES: OnceLock<Mutex<HashMap<i64, Arc<NativeHandle>>>> = OnceLock::new();
static NEXT_HANDLE: AtomicI64 = AtomicI64::new(1);

fn handles() -> &'static Mutex<HashMap<i64, Arc<NativeHandle>>> {
   HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_handle(id: jlong) -> Result<Arc<NativeHandle>> {
   handles()
      .lock()
      .map_err(|_| NativeError("native handle registry is poisoned".into()))?
      .get(&id)
      .cloned()
      .ok_or_else(|| NativeError(format!("unknown native handle {id}")))
}

fn create_handle() -> Result<jlong> {
   let runtime = Builder::new_current_thread()
      .enable_all()
      .build()
      .map_err(|error| NativeError(format!("failed to create Tokio runtime: {error}")))?;
   let http = pushcompat_listener::http_client_builder()
      .build()
      .map_err(|error| NativeError(format!("failed to create HTTP client: {error}")))?;
   let id = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
   if id <= 0 {
      return Err(NativeError("native handle space exhausted".into()));
   }
   handles()
      .lock()
      .map_err(|_| NativeError("native handle registry is poisoned".into()))?
      .insert(
         id,
         Arc::new(NativeHandle {
            http,
            runtime: Mutex::new(runtime),
            stop_requested: AtomicBool::new(false),
            stopper: Mutex::new(None),
         }),
      );
   Ok(id)
}

fn destroy_handle(id: jlong) -> Result<()> {
   let handle = handles()
      .lock()
      .map_err(|_| NativeError("native handle registry is poisoned".into()))?
      .remove(&id)
      .ok_or_else(|| NativeError(format!("unknown native handle {id}")))?;
   handle.request_stop()
}

fn check_in(id: jlong) -> Result<String> {
   let handle = get_handle(id)?;
   let runtime = handle
      .runtime
      .lock()
      .map_err(|_| NativeError("native runtime is poisoned".into()))?;
   let session = runtime
      .block_on(DeviceSession::fresh(&handle.http))
      .map_err(NativeError::from_display)?;
   serde_json::to_string(&session.into_state())
      .map_err(|error| NativeError(format!("failed to serialize device session: {error}")))
}

fn validate_json_size(value: &str, name: &str) -> Result<()> {
   if value.len() > MAX_JNI_JSON_BYTES {
      return Err(NativeError(format!("{name} exceeds the JNI size limit")));
   }
   Ok(())
}

fn register(id: jlong, session_json: &str, credentials_json: &str) -> Result<String> {
   validate_json_size(session_json, "device session JSON")?;
   validate_json_size(credentials_json, "FCM credentials JSON")?;
   let state = serde_json::from_str::<DeviceSessionState>(session_json)
      .map_err(|error| NativeError(format!("invalid device session JSON: {error}")))?;
   let credentials = serde_json::from_str::<FcmCredentials>(credentials_json)
      .map_err(|error| NativeError(format!("invalid FCM credentials JSON: {error}")))?;
   let handle = get_handle(id)?;
   let runtime = handle
      .runtime
      .lock()
      .map_err(|_| NativeError("native runtime is poisoned".into()))?;
   let registration = runtime
      .block_on(AppRegistration::register(
         &handle.http,
         &DeviceSession::restore(state),
         credentials,
      ))
      .map_err(NativeError::from_display)?;
   serde_json::to_string(&registration.into_state())
      .map_err(|error| NativeError(format!("failed to serialize FCM registration: {error}")))
}

fn read_string(env: &mut JNIEnv<'_>, value: &JString<'_>) -> Result<String> {
   env.get_string(value).map(Into::into).map_err(Into::into)
}

fn emit_callback(env: &mut JNIEnv<'_>, callback: &JObject<'_>, event: &Value) -> Result<()> {
   let event = serde_json::to_string(event)
      .map_err(|error| NativeError(format!("failed to serialize MCS event: {error}")))?;
   env.with_local_frame(4, |env| -> Result<()> {
      let event = env.new_string(event)?;
      env.call_method(callback, "accept", "(Ljava/lang/Object;)V", &[
         JValue::Object(event.as_ref()),
      ])?;
      Ok(())
   })
}

fn build_intent_payload(data: &DataMessage, sender_id: &str) -> Value {
   let mut fields = serde_json::Map::new();
   for (key, value) in &data.app_data {
      fields.insert(key.clone(), Value::String(value.clone()));
   }
   if let Some(raw) = &data.raw_data {
      match serde_json::from_slice::<Value>(raw) {
         Ok(Value::Object(raw_fields)) => {
            for (key, value) in raw_fields {
               fields.insert(
                  key,
                  Value::String(match value {
                     Value::String(value) => value,
                     value => value.to_string(),
                  }),
               );
            }
         },
         _ => {
            fields.insert(
               "message".into(),
               Value::String(String::from_utf8_lossy(raw).into_owned()),
            );
         },
      }
   }

   let message_id = data.id.clone().or_else(|| data.persistent_id.clone());
   for (key, value) in [
      ("google.message_id", message_id),
      (
         "from",
         Some(data.from.clone().unwrap_or_else(|| sender_id.to_owned())),
      ),
      ("google.c.sender.id", Some(sender_id.to_owned())),
      ("collapse_key", data.collapse_key.clone()),
   ] {
      if let Some(value) = value {
         fields.insert(key.into(), Value::String(value));
      }
   }
   Value::Object(fields)
}

async fn run_mcs(
   env: &mut JNIEnv<'_>,
   callback: &JObject<'_>,
   sessions_json: &str,
   registrations_json: &str,
   mut stop: oneshot::Receiver<()>,
) -> Result<()> {
   validate_json_size(sessions_json, "device sessions JSON")?;
   validate_json_size(registrations_json, "FCM registrations JSON")?;
   let mut sessions = serde_json::from_str::<HashMap<String, DeviceSessionState>>(sessions_json)
      .map_err(|error| NativeError(format!("invalid device sessions JSON: {error}")))?;
   let registrations = serde_json::from_str::<Vec<RunRegistration>>(registrations_json)
      .map_err(|error| NativeError(format!("invalid FCM registrations JSON: {error}")))?;
   if registrations.is_empty() {
      return Err(NativeError(
         "at least one FCM registration is required".into(),
      ));
   }
   if registrations.len() > MAX_MCS_IDENTITIES || sessions.len() > MAX_MCS_IDENTITIES {
      return Err(NativeError(format!(
         "on-device MCS supports at most {MAX_MCS_IDENTITIES} identities"
      )));
   }

   let mut senders = HashMap::with_capacity(registrations.len());
   let mut streams = StreamMap::new();
   for registration in registrations {
      let RunRegistration {
         state,
         persistent_ids,
      } = registration;
      if persistent_ids.len() > MAX_PERSISTENT_IDS {
         return Err(NativeError(format!(
            "FCM registration exceeds the {MAX_PERSISTENT_IDS}-ID acknowledgement limit"
         )));
      }
      let registration = AppRegistration::restore(state);
      let app_id = registration.credentials().package_name.clone();
      if app_id.is_empty() {
         return Err(NativeError("FCM registration package name is empty".into()));
      }
      if registration.credentials().sender_id.is_empty() {
         return Err(NativeError(format!("FCM sender ID is empty for {app_id}")));
      }
      if senders
         .insert(app_id.clone(), registration.credentials().sender_id.clone())
         .is_some()
      {
         return Err(NativeError(format!(
            "duplicate FCM registration for {app_id}"
         )));
      }
      let state = sessions
         .remove(&app_id)
         .ok_or_else(|| NativeError(format!("missing device session for {app_id}")))?;
      let session = DeviceSession::restore(state);
      let stream = tokio::select! {
         biased;
         _ = &mut stop => return Ok(()),
         result = session.connect(persistent_ids) => {
            result.map_err(NativeError::from_display)?
         },
      };
      streams.insert(app_id, NotifyClose::new(stream));
   }
   if !sessions.is_empty() {
      let mut unexpected = sessions.into_keys().collect::<Vec<_>>();
      unexpected.sort();
      return Err(NativeError(format!(
         "device sessions have no matching registration: {}",
         unexpected.join(", ")
      )));
   }

   emit_callback(env, callback, &json!({ "type": "connected" }))?;
   loop {
      let event = tokio::select! {
         biased;
         _ = &mut stop => return Ok(()),
         event = streams.next() => event,
      };
      let Some((app_id, event)) = event else {
         return Err(NativeError("all MCS streams ended".into()));
      };
      let Some(message) = event else {
         return Err(NativeError(format!("MCS stream ended for {app_id}")));
      };
      let message = message.map_err(NativeError::from_display)?;
      let stream = streams
         .iter_mut()
         .find(|(key, _)| key == &app_id)
         .map(|(_, stream)| stream)
         .ok_or_else(|| NativeError(format!("missing active MCS stream for {app_id}")))?
         .stream_mut()?;
      let stream_id = stream.last_stream_id_received();
      match message {
         Message::Data(data) => {
            let sender_id = senders
               .get(&app_id)
               .ok_or_else(|| NativeError(format!("missing FCM sender for {app_id}")))?;
            let event = json!({
               "type": "message",
               "app_id": app_id,
               "persistent_id": data.persistent_id,
               "payload": build_intent_payload(&data, sender_id),
            });
            emit_callback(env, callback, &event)?;
            let ack = new_stream_ack(stream_id);
            if ack.is_empty() {
               return Err(NativeError(
                  "failed to serialize MCS acknowledgement".into(),
               ));
            }
            stream.write_all(&ack).await.map_err(|error| {
               NativeError(format!("failed to acknowledge MCS message: {error}"))
            })?;
         },
         Message::HeartbeatPing => {
            let ack = new_heartbeat_ack(stream_id);
            if ack.is_empty() {
               return Err(NativeError(
                  "failed to serialize MCS heartbeat acknowledgement".into(),
               ));
            }
            stream.write_all(&ack).await.map_err(|error| {
               NativeError(format!("failed to acknowledge MCS heartbeat: {error}"))
            })?;
         },
         Message::Other(..) => {},
      }
   }
}

fn run(
   env: &mut JNIEnv<'_>,
   id: jlong,
   sessions_json: &str,
   registrations_json: &str,
   callback: &JObject<'_>,
) -> Result<()> {
   let handle = get_handle(id)?;
   let stop = handle.install_stopper()?;
   let _stopper_guard = StopperGuard(&handle);
   let runtime = handle
      .runtime
      .lock()
      .map_err(|_| NativeError("native runtime is poisoned".into()))?;
   runtime.block_on(run_mcs(
      env,
      callback,
      sessions_json,
      registrations_json,
      stop,
   ))
}

fn panic_error(payload: Box<dyn Any + Send>) -> NativeError {
   let message = payload
      .downcast_ref::<&str>()
      .copied()
      .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
      .unwrap_or("unknown panic");
   NativeError(format!("native listener panicked: {message}"))
}

fn catch_native<T>(operation: impl FnOnce() -> Result<T>) -> Result<T> {
   catch_unwind(AssertUnwindSafe(operation)).unwrap_or_else(|payload| Err(panic_error(payload)))
}

fn resolve_or_throw<T: Default>(env: &mut JNIEnv<'_>, result: Result<T>) -> T {
   match result {
      Ok(value) => value,
      Err(error) => {
         if !env.exception_check().unwrap_or(false) {
            let _ = env.throw_new("java/lang/RuntimeException", error.to_string());
         }
         T::default()
      },
   }
}

#[expect(unsafe_code, reason = "JNI export")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_benzeneos_pushcompat_NativeListener_nativeCreate<'local>(
   mut env: JNIEnv<'local>,
   _receiver: JObject<'local>,
) -> jlong {
   let result = catch_native(create_handle);
   resolve_or_throw(&mut env, result)
}

#[expect(unsafe_code, reason = "JNI export")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_benzeneos_pushcompat_NativeListener_nativeDestroy<'local>(
   mut env: JNIEnv<'local>,
   _receiver: JObject<'local>,
   handle: jlong,
) {
   let result = catch_native(|| destroy_handle(handle));
   resolve_or_throw(&mut env, result);
}

#[expect(unsafe_code, reason = "JNI export")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_benzeneos_pushcompat_NativeListener_nativeCheckIn<'local>(
   mut env: JNIEnv<'local>,
   _receiver: JObject<'local>,
   handle: jlong,
) -> jstring {
   let result = catch_native(|| {
      check_in(handle).and_then(|state| {
         env.new_string(state)
            .map(|state| state.into_raw())
            .map_err(Into::into)
      })
   });
   resolve_or_throw(&mut env, result)
}

#[expect(unsafe_code, reason = "JNI export")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_benzeneos_pushcompat_NativeListener_nativeRegister<'local>(
   mut env: JNIEnv<'local>,
   _receiver: JObject<'local>,
   handle: jlong,
   session_json: JString<'local>,
   credentials_json: JString<'local>,
) -> jstring {
   let result = catch_native(|| {
      let session_json = read_string(&mut env, &session_json)?;
      let credentials_json = read_string(&mut env, &credentials_json)?;
      let registration = register(handle, &session_json, &credentials_json)?;
      env.new_string(registration)
         .map(|registration| registration.into_raw())
         .map_err(Into::into)
   });
   resolve_or_throw(&mut env, result)
}

#[expect(unsafe_code, reason = "JNI export")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_benzeneos_pushcompat_NativeListener_nativeRun<'local>(
   mut env: JNIEnv<'local>,
   _receiver: JObject<'local>,
   handle: jlong,
   sessions_json: JString<'local>,
   registrations_json: JString<'local>,
   callback: JObject<'local>,
) {
   let result = catch_native(|| {
      let sessions_json = read_string(&mut env, &sessions_json)?;
      let registrations_json = read_string(&mut env, &registrations_json)?;
      run(
         &mut env,
         handle,
         &sessions_json,
         &registrations_json,
         &callback,
      )
   });
   resolve_or_throw(&mut env, result);
}

#[expect(unsafe_code, reason = "JNI export")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_benzeneos_pushcompat_NativeListener_nativeStop<'local>(
   mut env: JNIEnv<'local>,
   _receiver: JObject<'local>,
   handle: jlong,
) {
   let result = catch_native(|| get_handle(handle)?.request_stop());
   resolve_or_throw(&mut env, result);
}
