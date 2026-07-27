// Automatically generated rust module for 'mcs.proto' file
// Regenerate from the repository root: nix develop -c bash nix/regen-proto.sh

#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unknown_lints)]
#![allow(clippy::all)]
#![cfg_attr(rustfmt, rustfmt_skip)]


use quick_protobuf::{MessageInfo, MessageRead, MessageWrite, BytesReader, Writer, WriterBackend, Result};
use quick_protobuf::sizeofs::{sizeof_varint, sizeof_len};
use super::*;

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct HeartbeatPing {
    pub stream_id: Option<i32>,
    pub last_stream_id_received: Option<i32>,
    pub status: Option<i64>,
}

impl<'a> MessageRead<'a> for HeartbeatPing {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(8) => msg.stream_id = Some(r.read_int32(bytes)?),
                Ok(16) => msg.last_stream_id_received = Some(r.read_int32(bytes)?),
                Ok(24) => msg.status = Some(r.read_int64(bytes)?),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for HeartbeatPing {
    fn get_size(&self) -> usize {
        0
        + self.stream_id.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.last_stream_id_received.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.status.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        if let Some(ref s) = self.stream_id { w.write_with_tag(8, |w| w.write_int32(*s))?; }
        if let Some(ref s) = self.last_stream_id_received { w.write_with_tag(16, |w| w.write_int32(*s))?; }
        if let Some(ref s) = self.status { w.write_with_tag(24, |w| w.write_int64(*s))?; }
        Ok(())
    }
}

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct HeartbeatAck {
    pub stream_id: Option<i32>,
    pub last_stream_id_received: Option<i32>,
    pub status: Option<i64>,
}

impl<'a> MessageRead<'a> for HeartbeatAck {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(8) => msg.stream_id = Some(r.read_int32(bytes)?),
                Ok(16) => msg.last_stream_id_received = Some(r.read_int32(bytes)?),
                Ok(24) => msg.status = Some(r.read_int64(bytes)?),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for HeartbeatAck {
    fn get_size(&self) -> usize {
        0
        + self.stream_id.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.last_stream_id_received.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.status.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        if let Some(ref s) = self.stream_id { w.write_with_tag(8, |w| w.write_int32(*s))?; }
        if let Some(ref s) = self.last_stream_id_received { w.write_with_tag(16, |w| w.write_int32(*s))?; }
        if let Some(ref s) = self.status { w.write_with_tag(24, |w| w.write_int64(*s))?; }
        Ok(())
    }
}

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct ErrorInfo {
    pub code: i32,
    pub message: Option<String>,
    pub type_pb: Option<String>,
    pub extension: Option<Extension>,
}

impl<'a> MessageRead<'a> for ErrorInfo {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(8) => msg.code = r.read_int32(bytes)?,
                Ok(18) => msg.message = Some(r.read_string(bytes)?.to_owned()),
                Ok(26) => msg.type_pb = Some(r.read_string(bytes)?.to_owned()),
                Ok(34) => msg.extension = Some(r.read_message::<Extension>(bytes)?),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for ErrorInfo {
    fn get_size(&self) -> usize {
        0
        + 1 + sizeof_varint(*(&self.code) as u64)
        + self.message.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.type_pb.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.extension.as_ref().map_or(0, |m| 1 + sizeof_len((m).get_size()))
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        w.write_with_tag(8, |w| w.write_int32(*&self.code))?;
        if let Some(ref s) = self.message { w.write_with_tag(18, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.type_pb { w.write_with_tag(26, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.extension { w.write_with_tag(34, |w| w.write_message(s))?; }
        Ok(())
    }
}

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct Setting {
    pub name: String,
    pub value: String,
}

impl<'a> MessageRead<'a> for Setting {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(10) => msg.name = r.read_string(bytes)?.to_owned(),
                Ok(18) => msg.value = r.read_string(bytes)?.to_owned(),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for Setting {
    fn get_size(&self) -> usize {
        0
        + 1 + sizeof_len((&self.name).len())
        + 1 + sizeof_len((&self.value).len())
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        w.write_with_tag(10, |w| w.write_string(&**&self.name))?;
        w.write_with_tag(18, |w| w.write_string(&**&self.value))?;
        Ok(())
    }
}

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct HeartbeatStat {
    pub ip: String,
    pub timeout: bool,
    pub interval_ms: i32,
}

impl<'a> MessageRead<'a> for HeartbeatStat {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(10) => msg.ip = r.read_string(bytes)?.to_owned(),
                Ok(16) => msg.timeout = r.read_bool(bytes)?,
                Ok(24) => msg.interval_ms = r.read_int32(bytes)?,
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for HeartbeatStat {
    fn get_size(&self) -> usize {
        0
        + 1 + sizeof_len((&self.ip).len())
        + 1 + sizeof_varint(u64::from(*(&self.timeout)))
        + 1 + sizeof_varint(*(&self.interval_ms) as u64)
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        w.write_with_tag(10, |w| w.write_string(&**&self.ip))?;
        w.write_with_tag(16, |w| w.write_bool(*&self.timeout))?;
        w.write_with_tag(24, |w| w.write_int32(*&self.interval_ms))?;
        Ok(())
    }
}

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct HeartbeatConfig {
    pub upload_stat: Option<bool>,
    pub ip: Option<String>,
    pub interval_ms: Option<i32>,
}

impl<'a> MessageRead<'a> for HeartbeatConfig {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(8) => msg.upload_stat = Some(r.read_bool(bytes)?),
                Ok(18) => msg.ip = Some(r.read_string(bytes)?.to_owned()),
                Ok(24) => msg.interval_ms = Some(r.read_int32(bytes)?),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for HeartbeatConfig {
    fn get_size(&self) -> usize {
        0
        + self.upload_stat.as_ref().map_or(0, |m| 1 + sizeof_varint(u64::from(*(m))))
        + self.ip.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.interval_ms.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        if let Some(ref s) = self.upload_stat { w.write_with_tag(8, |w| w.write_bool(*s))?; }
        if let Some(ref s) = self.ip { w.write_with_tag(18, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.interval_ms { w.write_with_tag(24, |w| w.write_int32(*s))?; }
        Ok(())
    }
}

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct ClientEvent {
    pub type_pb: Option<mod_ClientEvent::Type>,
    pub number_discarded_events: Option<u32>,
    pub network_type: Option<i32>,
    pub time_connection_started_ms: Option<u64>,
    pub time_connection_ended_ms: Option<u64>,
    pub error_code: Option<i32>,
    pub time_connection_established_ms: Option<u64>,
}

impl<'a> MessageRead<'a> for ClientEvent {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(8) => msg.type_pb = Some(r.read_enum(bytes)?),
                Ok(800) => msg.number_discarded_events = Some(r.read_uint32(bytes)?),
                Ok(1600) => msg.network_type = Some(r.read_int32(bytes)?),
                Ok(1616) => msg.time_connection_started_ms = Some(r.read_uint64(bytes)?),
                Ok(1624) => msg.time_connection_ended_ms = Some(r.read_uint64(bytes)?),
                Ok(1632) => msg.error_code = Some(r.read_int32(bytes)?),
                Ok(2400) => msg.time_connection_established_ms = Some(r.read_uint64(bytes)?),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for ClientEvent {
    fn get_size(&self) -> usize {
        0
        + self.type_pb.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.number_discarded_events.as_ref().map_or(0, |m| 2 + sizeof_varint(u64::from(*(m))))
        + self.network_type.as_ref().map_or(0, |m| 2 + sizeof_varint(*(m) as u64))
        + self.time_connection_started_ms.as_ref().map_or(0, |m| 2 + sizeof_varint(*(m) as u64))
        + self.time_connection_ended_ms.as_ref().map_or(0, |m| 2 + sizeof_varint(*(m) as u64))
        + self.error_code.as_ref().map_or(0, |m| 2 + sizeof_varint(*(m) as u64))
        + self.time_connection_established_ms.as_ref().map_or(0, |m| 2 + sizeof_varint(*(m) as u64))
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        if let Some(ref s) = self.type_pb { w.write_with_tag(8, |w| w.write_enum(*s as i32))?; }
        if let Some(ref s) = self.number_discarded_events { w.write_with_tag(800, |w| w.write_uint32(*s))?; }
        if let Some(ref s) = self.network_type { w.write_with_tag(1600, |w| w.write_int32(*s))?; }
        if let Some(ref s) = self.time_connection_started_ms { w.write_with_tag(1616, |w| w.write_uint64(*s))?; }
        if let Some(ref s) = self.time_connection_ended_ms { w.write_with_tag(1624, |w| w.write_uint64(*s))?; }
        if let Some(ref s) = self.error_code { w.write_with_tag(1632, |w| w.write_int32(*s))?; }
        if let Some(ref s) = self.time_connection_established_ms { w.write_with_tag(2400, |w| w.write_uint64(*s))?; }
        Ok(())
    }
}

pub mod mod_ClientEvent {


#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Type {
    UNKNOWN = 0,
    DISCARDED_EVENTS = 1,
    FAILED_CONNECTION = 2,
    SUCCESSFUL_CONNECTION = 3,
}

impl Default for Type {
    fn default() -> Self {
        Self::UNKNOWN
    }
}

impl From<i32> for Type {
    fn from(i: i32) -> Self {
        match i {
            0 => Self::UNKNOWN,
            1 => Self::DISCARDED_EVENTS,
            2 => Self::FAILED_CONNECTION,
            3 => Self::SUCCESSFUL_CONNECTION,
            _ => Self::default(),
        }
    }
}

impl<'a> From<&'a str> for Type {
    fn from(s: &'a str) -> Self {
        match s {
            "UNKNOWN" => Self::UNKNOWN,
            "DISCARDED_EVENTS" => Self::DISCARDED_EVENTS,
            "FAILED_CONNECTION" => Self::FAILED_CONNECTION,
            "SUCCESSFUL_CONNECTION" => Self::SUCCESSFUL_CONNECTION,
            _ => Self::default(),
        }
    }
}

}

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct LoginRequest {
    pub id: String,
    pub domain: String,
    pub user: String,
    pub resource: String,
    pub auth_token: String,
    pub device_id: Option<String>,
    pub last_rmq_id: Option<i64>,
    pub setting: Vec<Setting>,
    pub received_persistent_id: Vec<String>,
    pub adaptive_heartbeat: Option<bool>,
    pub heartbeat_stat: Option<HeartbeatStat>,
    pub use_rmq2: Option<bool>,
    pub account_id: Option<i64>,
    pub auth_service: Option<mod_LoginRequest::AuthService>,
    pub network_type: Option<i32>,
    pub status: Option<i64>,
    pub client_event: Vec<ClientEvent>,
}

impl<'a> MessageRead<'a> for LoginRequest {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(10) => msg.id = r.read_string(bytes)?.to_owned(),
                Ok(18) => msg.domain = r.read_string(bytes)?.to_owned(),
                Ok(26) => msg.user = r.read_string(bytes)?.to_owned(),
                Ok(34) => msg.resource = r.read_string(bytes)?.to_owned(),
                Ok(42) => msg.auth_token = r.read_string(bytes)?.to_owned(),
                Ok(50) => msg.device_id = Some(r.read_string(bytes)?.to_owned()),
                Ok(56) => msg.last_rmq_id = Some(r.read_int64(bytes)?),
                Ok(66) => msg.setting.push(r.read_message::<Setting>(bytes)?),
                Ok(82) => msg.received_persistent_id.push(r.read_string(bytes)?.to_owned()),
                Ok(96) => msg.adaptive_heartbeat = Some(r.read_bool(bytes)?),
                Ok(106) => msg.heartbeat_stat = Some(r.read_message::<HeartbeatStat>(bytes)?),
                Ok(112) => msg.use_rmq2 = Some(r.read_bool(bytes)?),
                Ok(120) => msg.account_id = Some(r.read_int64(bytes)?),
                Ok(128) => msg.auth_service = Some(r.read_enum(bytes)?),
                Ok(136) => msg.network_type = Some(r.read_int32(bytes)?),
                Ok(144) => msg.status = Some(r.read_int64(bytes)?),
                Ok(178) => msg.client_event.push(r.read_message::<ClientEvent>(bytes)?),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for LoginRequest {
    fn get_size(&self) -> usize {
        0
        + 1 + sizeof_len((&self.id).len())
        + 1 + sizeof_len((&self.domain).len())
        + 1 + sizeof_len((&self.user).len())
        + 1 + sizeof_len((&self.resource).len())
        + 1 + sizeof_len((&self.auth_token).len())
        + self.device_id.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.last_rmq_id.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.setting.iter().map(|s| 1 + sizeof_len((s).get_size())).sum::<usize>()
        + self.received_persistent_id.iter().map(|s| 1 + sizeof_len((s).len())).sum::<usize>()
        + self.adaptive_heartbeat.as_ref().map_or(0, |m| 1 + sizeof_varint(u64::from(*(m))))
        + self.heartbeat_stat.as_ref().map_or(0, |m| 1 + sizeof_len((m).get_size()))
        + self.use_rmq2.as_ref().map_or(0, |m| 1 + sizeof_varint(u64::from(*(m))))
        + self.account_id.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.auth_service.as_ref().map_or(0, |m| 2 + sizeof_varint(*(m) as u64))
        + self.network_type.as_ref().map_or(0, |m| 2 + sizeof_varint(*(m) as u64))
        + self.status.as_ref().map_or(0, |m| 2 + sizeof_varint(*(m) as u64))
        + self.client_event.iter().map(|s| 2 + sizeof_len((s).get_size())).sum::<usize>()
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        w.write_with_tag(10, |w| w.write_string(&**&self.id))?;
        w.write_with_tag(18, |w| w.write_string(&**&self.domain))?;
        w.write_with_tag(26, |w| w.write_string(&**&self.user))?;
        w.write_with_tag(34, |w| w.write_string(&**&self.resource))?;
        w.write_with_tag(42, |w| w.write_string(&**&self.auth_token))?;
        if let Some(ref s) = self.device_id { w.write_with_tag(50, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.last_rmq_id { w.write_with_tag(56, |w| w.write_int64(*s))?; }
        for s in &self.setting { w.write_with_tag(66, |w| w.write_message(s))?; }
        for s in &self.received_persistent_id { w.write_with_tag(82, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.adaptive_heartbeat { w.write_with_tag(96, |w| w.write_bool(*s))?; }
        if let Some(ref s) = self.heartbeat_stat { w.write_with_tag(106, |w| w.write_message(s))?; }
        if let Some(ref s) = self.use_rmq2 { w.write_with_tag(112, |w| w.write_bool(*s))?; }
        if let Some(ref s) = self.account_id { w.write_with_tag(120, |w| w.write_int64(*s))?; }
        if let Some(ref s) = self.auth_service { w.write_with_tag(128, |w| w.write_enum(*s as i32))?; }
        if let Some(ref s) = self.network_type { w.write_with_tag(136, |w| w.write_int32(*s))?; }
        if let Some(ref s) = self.status { w.write_with_tag(144, |w| w.write_int64(*s))?; }
        for s in &self.client_event { w.write_with_tag(178, |w| w.write_message(s))?; }
        Ok(())
    }
}

pub mod mod_LoginRequest {


#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AuthService {
    ANDROID_ID = 2,
}

impl Default for AuthService {
    fn default() -> Self {
        Self::ANDROID_ID
    }
}

impl From<i32> for AuthService {
    fn from(i: i32) -> Self {
        match i {
            2 => Self::ANDROID_ID,
            _ => Self::default(),
        }
    }
}

impl<'a> From<&'a str> for AuthService {
    fn from(s: &'a str) -> Self {
        match s {
            "ANDROID_ID" => Self::ANDROID_ID,
            _ => Self::default(),
        }
    }
}

}

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct LoginResponse {
    pub id: String,
    pub jid: Option<String>,
    pub error: Option<ErrorInfo>,
    pub setting: Vec<Setting>,
    pub stream_id: Option<i32>,
    pub last_stream_id_received: Option<i32>,
    pub heartbeat_config: Option<HeartbeatConfig>,
    pub server_timestamp: Option<i64>,
}

impl<'a> MessageRead<'a> for LoginResponse {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(10) => msg.id = r.read_string(bytes)?.to_owned(),
                Ok(18) => msg.jid = Some(r.read_string(bytes)?.to_owned()),
                Ok(26) => msg.error = Some(r.read_message::<ErrorInfo>(bytes)?),
                Ok(34) => msg.setting.push(r.read_message::<Setting>(bytes)?),
                Ok(40) => msg.stream_id = Some(r.read_int32(bytes)?),
                Ok(48) => msg.last_stream_id_received = Some(r.read_int32(bytes)?),
                Ok(58) => msg.heartbeat_config = Some(r.read_message::<HeartbeatConfig>(bytes)?),
                Ok(64) => msg.server_timestamp = Some(r.read_int64(bytes)?),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for LoginResponse {
    fn get_size(&self) -> usize {
        0
        + 1 + sizeof_len((&self.id).len())
        + self.jid.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.error.as_ref().map_or(0, |m| 1 + sizeof_len((m).get_size()))
        + self.setting.iter().map(|s| 1 + sizeof_len((s).get_size())).sum::<usize>()
        + self.stream_id.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.last_stream_id_received.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.heartbeat_config.as_ref().map_or(0, |m| 1 + sizeof_len((m).get_size()))
        + self.server_timestamp.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        w.write_with_tag(10, |w| w.write_string(&**&self.id))?;
        if let Some(ref s) = self.jid { w.write_with_tag(18, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.error { w.write_with_tag(26, |w| w.write_message(s))?; }
        for s in &self.setting { w.write_with_tag(34, |w| w.write_message(s))?; }
        if let Some(ref s) = self.stream_id { w.write_with_tag(40, |w| w.write_int32(*s))?; }
        if let Some(ref s) = self.last_stream_id_received { w.write_with_tag(48, |w| w.write_int32(*s))?; }
        if let Some(ref s) = self.heartbeat_config { w.write_with_tag(58, |w| w.write_message(s))?; }
        if let Some(ref s) = self.server_timestamp { w.write_with_tag(64, |w| w.write_int64(*s))?; }
        Ok(())
    }
}

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct StreamErrorStanza {
    pub type_pb: String,
    pub text: Option<String>,
}

impl<'a> MessageRead<'a> for StreamErrorStanza {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(10) => msg.type_pb = r.read_string(bytes)?.to_owned(),
                Ok(18) => msg.text = Some(r.read_string(bytes)?.to_owned()),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for StreamErrorStanza {
    fn get_size(&self) -> usize {
        0
        + 1 + sizeof_len((&self.type_pb).len())
        + self.text.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        w.write_with_tag(10, |w| w.write_string(&**&self.type_pb))?;
        if let Some(ref s) = self.text { w.write_with_tag(18, |w| w.write_string(&**s))?; }
        Ok(())
    }
}

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct Close { }

impl MessageRead<'_> for Close {
    fn from_reader(r: &mut BytesReader, _: &[u8]) -> Result<Self> {
        r.read_to_end();
        Ok(Self::default())
    }
}

impl MessageWrite for Close { }

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct Extension {
    pub id: i32,
    pub data: Vec<u8>,
}

impl<'a> MessageRead<'a> for Extension {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(8) => msg.id = r.read_int32(bytes)?,
                Ok(18) => msg.data = r.read_bytes(bytes)?.to_owned(),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for Extension {
    fn get_size(&self) -> usize {
        0
        + 1 + sizeof_varint(*(&self.id) as u64)
        + 1 + sizeof_len((&self.data).len())
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        w.write_with_tag(8, |w| w.write_int32(*&self.id))?;
        w.write_with_tag(18, |w| w.write_bytes(&**&self.data))?;
        Ok(())
    }
}

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct IqStanza {
    pub rmq_id: Option<i64>,
    pub type_pb: mod_IqStanza::IqType,
    pub id: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub error: Option<ErrorInfo>,
    pub extension: Option<Extension>,
    pub persistent_id: Option<String>,
    pub stream_id: Option<i32>,
    pub last_stream_id_received: Option<i32>,
    pub account_id: Option<i64>,
    pub status: Option<i64>,
}

impl<'a> MessageRead<'a> for IqStanza {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(8) => msg.rmq_id = Some(r.read_int64(bytes)?),
                Ok(16) => msg.type_pb = r.read_enum(bytes)?,
                Ok(26) => msg.id = r.read_string(bytes)?.to_owned(),
                Ok(34) => msg.from = Some(r.read_string(bytes)?.to_owned()),
                Ok(42) => msg.to = Some(r.read_string(bytes)?.to_owned()),
                Ok(50) => msg.error = Some(r.read_message::<ErrorInfo>(bytes)?),
                Ok(58) => msg.extension = Some(r.read_message::<Extension>(bytes)?),
                Ok(66) => msg.persistent_id = Some(r.read_string(bytes)?.to_owned()),
                Ok(72) => msg.stream_id = Some(r.read_int32(bytes)?),
                Ok(80) => msg.last_stream_id_received = Some(r.read_int32(bytes)?),
                Ok(88) => msg.account_id = Some(r.read_int64(bytes)?),
                Ok(96) => msg.status = Some(r.read_int64(bytes)?),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for IqStanza {
    fn get_size(&self) -> usize {
        0
        + self.rmq_id.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + 1 + sizeof_varint(*(&self.type_pb) as u64)
        + 1 + sizeof_len((&self.id).len())
        + self.from.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.to.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.error.as_ref().map_or(0, |m| 1 + sizeof_len((m).get_size()))
        + self.extension.as_ref().map_or(0, |m| 1 + sizeof_len((m).get_size()))
        + self.persistent_id.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.stream_id.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.last_stream_id_received.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.account_id.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.status.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        if let Some(ref s) = self.rmq_id { w.write_with_tag(8, |w| w.write_int64(*s))?; }
        w.write_with_tag(16, |w| w.write_enum(*&self.type_pb as i32))?;
        w.write_with_tag(26, |w| w.write_string(&**&self.id))?;
        if let Some(ref s) = self.from { w.write_with_tag(34, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.to { w.write_with_tag(42, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.error { w.write_with_tag(50, |w| w.write_message(s))?; }
        if let Some(ref s) = self.extension { w.write_with_tag(58, |w| w.write_message(s))?; }
        if let Some(ref s) = self.persistent_id { w.write_with_tag(66, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.stream_id { w.write_with_tag(72, |w| w.write_int32(*s))?; }
        if let Some(ref s) = self.last_stream_id_received { w.write_with_tag(80, |w| w.write_int32(*s))?; }
        if let Some(ref s) = self.account_id { w.write_with_tag(88, |w| w.write_int64(*s))?; }
        if let Some(ref s) = self.status { w.write_with_tag(96, |w| w.write_int64(*s))?; }
        Ok(())
    }
}

pub mod mod_IqStanza {


#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum IqType {
    GET = 0,
    SET = 1,
    RESULT = 2,
    IQ_ERROR = 3,
}

impl Default for IqType {
    fn default() -> Self {
        Self::GET
    }
}

impl From<i32> for IqType {
    fn from(i: i32) -> Self {
        match i {
            0 => Self::GET,
            1 => Self::SET,
            2 => Self::RESULT,
            3 => Self::IQ_ERROR,
            _ => Self::default(),
        }
    }
}

impl<'a> From<&'a str> for IqType {
    fn from(s: &'a str) -> Self {
        match s {
            "GET" => Self::GET,
            "SET" => Self::SET,
            "RESULT" => Self::RESULT,
            "IQ_ERROR" => Self::IQ_ERROR,
            _ => Self::default(),
        }
    }
}

}

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct AppData {
    pub key: String,
    pub value: String,
}

impl<'a> MessageRead<'a> for AppData {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(10) => msg.key = r.read_string(bytes)?.to_owned(),
                Ok(18) => msg.value = r.read_string(bytes)?.to_owned(),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for AppData {
    fn get_size(&self) -> usize {
        0
        + 1 + sizeof_len((&self.key).len())
        + 1 + sizeof_len((&self.value).len())
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        w.write_with_tag(10, |w| w.write_string(&**&self.key))?;
        w.write_with_tag(18, |w| w.write_string(&**&self.value))?;
        Ok(())
    }
}

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct DataMessageStanza {
    pub id: Option<String>,
    pub from: String,
    pub to: Option<String>,
    pub category: String,
    pub token: Option<String>,
    pub app_data: Vec<AppData>,
    pub from_trusted_server: Option<bool>,
    pub persistent_id: Option<String>,
    pub stream_id: Option<i32>,
    pub last_stream_id_received: Option<i32>,
    pub reg_id: Option<String>,
    pub device_user_id: Option<i64>,
    pub ttl: Option<i32>,
    pub sent: Option<i64>,
    pub queued: Option<i32>,
    pub status: Option<i64>,
    pub raw_data: Option<Vec<u8>>,
    pub immediate_ack: Option<bool>,
}

impl<'a> MessageRead<'a> for DataMessageStanza {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(18) => msg.id = Some(r.read_string(bytes)?.to_owned()),
                Ok(26) => msg.from = r.read_string(bytes)?.to_owned(),
                Ok(34) => msg.to = Some(r.read_string(bytes)?.to_owned()),
                Ok(42) => msg.category = r.read_string(bytes)?.to_owned(),
                Ok(50) => msg.token = Some(r.read_string(bytes)?.to_owned()),
                Ok(58) => msg.app_data.push(r.read_message::<AppData>(bytes)?),
                Ok(64) => msg.from_trusted_server = Some(r.read_bool(bytes)?),
                Ok(74) => msg.persistent_id = Some(r.read_string(bytes)?.to_owned()),
                Ok(80) => msg.stream_id = Some(r.read_int32(bytes)?),
                Ok(88) => msg.last_stream_id_received = Some(r.read_int32(bytes)?),
                Ok(106) => msg.reg_id = Some(r.read_string(bytes)?.to_owned()),
                Ok(128) => msg.device_user_id = Some(r.read_int64(bytes)?),
                Ok(136) => msg.ttl = Some(r.read_int32(bytes)?),
                Ok(144) => msg.sent = Some(r.read_int64(bytes)?),
                Ok(152) => msg.queued = Some(r.read_int32(bytes)?),
                Ok(160) => msg.status = Some(r.read_int64(bytes)?),
                Ok(170) => msg.raw_data = Some(r.read_bytes(bytes)?.to_owned()),
                Ok(192) => msg.immediate_ack = Some(r.read_bool(bytes)?),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for DataMessageStanza {
    fn get_size(&self) -> usize {
        0
        + self.id.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + 1 + sizeof_len((&self.from).len())
        + self.to.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + 1 + sizeof_len((&self.category).len())
        + self.token.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.app_data.iter().map(|s| 1 + sizeof_len((s).get_size())).sum::<usize>()
        + self.from_trusted_server.as_ref().map_or(0, |m| 1 + sizeof_varint(u64::from(*(m))))
        + self.persistent_id.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.stream_id.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.last_stream_id_received.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.reg_id.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.device_user_id.as_ref().map_or(0, |m| 2 + sizeof_varint(*(m) as u64))
        + self.ttl.as_ref().map_or(0, |m| 2 + sizeof_varint(*(m) as u64))
        + self.sent.as_ref().map_or(0, |m| 2 + sizeof_varint(*(m) as u64))
        + self.queued.as_ref().map_or(0, |m| 2 + sizeof_varint(*(m) as u64))
        + self.status.as_ref().map_or(0, |m| 2 + sizeof_varint(*(m) as u64))
        + self.raw_data.as_ref().map_or(0, |m| 2 + sizeof_len((m).len()))
        + self.immediate_ack.as_ref().map_or(0, |m| 2 + sizeof_varint(u64::from(*(m))))
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        if let Some(ref s) = self.id { w.write_with_tag(18, |w| w.write_string(&**s))?; }
        w.write_with_tag(26, |w| w.write_string(&**&self.from))?;
        if let Some(ref s) = self.to { w.write_with_tag(34, |w| w.write_string(&**s))?; }
        w.write_with_tag(42, |w| w.write_string(&**&self.category))?;
        if let Some(ref s) = self.token { w.write_with_tag(50, |w| w.write_string(&**s))?; }
        for s in &self.app_data { w.write_with_tag(58, |w| w.write_message(s))?; }
        if let Some(ref s) = self.from_trusted_server { w.write_with_tag(64, |w| w.write_bool(*s))?; }
        if let Some(ref s) = self.persistent_id { w.write_with_tag(74, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.stream_id { w.write_with_tag(80, |w| w.write_int32(*s))?; }
        if let Some(ref s) = self.last_stream_id_received { w.write_with_tag(88, |w| w.write_int32(*s))?; }
        if let Some(ref s) = self.reg_id { w.write_with_tag(106, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.device_user_id { w.write_with_tag(128, |w| w.write_int64(*s))?; }
        if let Some(ref s) = self.ttl { w.write_with_tag(136, |w| w.write_int32(*s))?; }
        if let Some(ref s) = self.sent { w.write_with_tag(144, |w| w.write_int64(*s))?; }
        if let Some(ref s) = self.queued { w.write_with_tag(152, |w| w.write_int32(*s))?; }
        if let Some(ref s) = self.status { w.write_with_tag(160, |w| w.write_int64(*s))?; }
        if let Some(ref s) = self.raw_data { w.write_with_tag(170, |w| w.write_bytes(&**s))?; }
        if let Some(ref s) = self.immediate_ack { w.write_with_tag(192, |w| w.write_bool(*s))?; }
        Ok(())
    }
}

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct StreamAck { }

impl MessageRead<'_> for StreamAck {
    fn from_reader(r: &mut BytesReader, _: &[u8]) -> Result<Self> {
        r.read_to_end();
        Ok(Self::default())
    }
}

impl MessageWrite for StreamAck { }

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct SelectiveAck {
    pub id: Vec<String>,
}

impl<'a> MessageRead<'a> for SelectiveAck {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(10) => msg.id.push(r.read_string(bytes)?.to_owned()),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for SelectiveAck {
    fn get_size(&self) -> usize {
        0
        + self.id.iter().map(|s| 1 + sizeof_len((s).len())).sum::<usize>()
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        for s in &self.id { w.write_with_tag(10, |w| w.write_string(&**s))?; }
        Ok(())
    }
}

