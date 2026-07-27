// Automatically generated rust module for 'android_checkin.proto' file
// Regenerate from the repository root: nix develop -c bash nix/regen-proto.sh

#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(unused_imports)]
#![allow(unknown_lints)]
#![allow(clippy::all)]
#![cfg_attr(rustfmt, rustfmt_skip)]


use quick_protobuf::{MessageInfo, MessageRead, MessageWrite, BytesReader, Writer, WriterBackend, Result};
use quick_protobuf::sizeofs::{sizeof_varint, sizeof_len};
use super::*;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum DeviceType {
    DEVICE_ANDROID_OS = 1,
    DEVICE_IOS_OS = 2,
    DEVICE_CHROME_BROWSER = 3,
    DEVICE_CHROME_OS = 4,
}

impl Default for DeviceType {
    fn default() -> Self {
        Self::DEVICE_ANDROID_OS
    }
}

impl From<i32> for DeviceType {
    fn from(i: i32) -> Self {
        match i {
            1 => Self::DEVICE_ANDROID_OS,
            2 => Self::DEVICE_IOS_OS,
            3 => Self::DEVICE_CHROME_BROWSER,
            4 => Self::DEVICE_CHROME_OS,
            _ => Self::default(),
        }
    }
}

impl<'a> From<&'a str> for DeviceType {
    fn from(s: &'a str) -> Self {
        match s {
            "DEVICE_ANDROID_OS" => Self::DEVICE_ANDROID_OS,
            "DEVICE_IOS_OS" => Self::DEVICE_IOS_OS,
            "DEVICE_CHROME_BROWSER" => Self::DEVICE_CHROME_BROWSER,
            "DEVICE_CHROME_OS" => Self::DEVICE_CHROME_OS,
            _ => Self::default(),
        }
    }
}

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct ChromeBuildProto {
    pub platform: Option<mod_ChromeBuildProto::Platform>,
    pub chrome_version: Option<String>,
    pub channel: Option<mod_ChromeBuildProto::Channel>,
}

impl<'a> MessageRead<'a> for ChromeBuildProto {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(8) => msg.platform = Some(r.read_enum(bytes)?),
                Ok(18) => msg.chrome_version = Some(r.read_string(bytes)?.to_owned()),
                Ok(24) => msg.channel = Some(r.read_enum(bytes)?),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for ChromeBuildProto {
    fn get_size(&self) -> usize {
        0
        + self.platform.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.chrome_version.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.channel.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        if let Some(ref s) = self.platform { w.write_with_tag(8, |w| w.write_enum(*s as i32))?; }
        if let Some(ref s) = self.chrome_version { w.write_with_tag(18, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.channel { w.write_with_tag(24, |w| w.write_enum(*s as i32))?; }
        Ok(())
    }
}

pub mod mod_ChromeBuildProto {


#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Platform {
    PLATFORM_WIN = 1,
    PLATFORM_MAC = 2,
    PLATFORM_LINUX = 3,
    PLATFORM_CROS = 4,
    PLATFORM_IOS = 5,
    PLATFORM_ANDROID = 6,
}

impl Default for Platform {
    fn default() -> Self {
        Self::PLATFORM_WIN
    }
}

impl From<i32> for Platform {
    fn from(i: i32) -> Self {
        match i {
            1 => Self::PLATFORM_WIN,
            2 => Self::PLATFORM_MAC,
            3 => Self::PLATFORM_LINUX,
            4 => Self::PLATFORM_CROS,
            5 => Self::PLATFORM_IOS,
            6 => Self::PLATFORM_ANDROID,
            _ => Self::default(),
        }
    }
}

impl<'a> From<&'a str> for Platform {
    fn from(s: &'a str) -> Self {
        match s {
            "PLATFORM_WIN" => Self::PLATFORM_WIN,
            "PLATFORM_MAC" => Self::PLATFORM_MAC,
            "PLATFORM_LINUX" => Self::PLATFORM_LINUX,
            "PLATFORM_CROS" => Self::PLATFORM_CROS,
            "PLATFORM_IOS" => Self::PLATFORM_IOS,
            "PLATFORM_ANDROID" => Self::PLATFORM_ANDROID,
            _ => Self::default(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Channel {
    CHANNEL_STABLE = 1,
    CHANNEL_BETA = 2,
    CHANNEL_DEV = 3,
    CHANNEL_CANARY = 4,
    CHANNEL_UNKNOWN = 5,
}

impl Default for Channel {
    fn default() -> Self {
        Self::CHANNEL_STABLE
    }
}

impl From<i32> for Channel {
    fn from(i: i32) -> Self {
        match i {
            1 => Self::CHANNEL_STABLE,
            2 => Self::CHANNEL_BETA,
            3 => Self::CHANNEL_DEV,
            4 => Self::CHANNEL_CANARY,
            5 => Self::CHANNEL_UNKNOWN,
            _ => Self::default(),
        }
    }
}

impl<'a> From<&'a str> for Channel {
    fn from(s: &'a str) -> Self {
        match s {
            "CHANNEL_STABLE" => Self::CHANNEL_STABLE,
            "CHANNEL_BETA" => Self::CHANNEL_BETA,
            "CHANNEL_DEV" => Self::CHANNEL_DEV,
            "CHANNEL_CANARY" => Self::CHANNEL_CANARY,
            "CHANNEL_UNKNOWN" => Self::CHANNEL_UNKNOWN,
            _ => Self::default(),
        }
    }
}

}

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct AndroidBuildProto {
    pub fingerprint: Option<String>,
    pub hardware: Option<String>,
    pub brand: Option<String>,
    pub radio: Option<String>,
    pub bootloader: Option<String>,
    pub client_id: Option<String>,
    pub time: Option<i64>,
    pub package_version_code: Option<i32>,
    pub device: Option<String>,
    pub sdk_version: Option<i32>,
    pub model: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub ota_installed: Option<bool>,
}

impl<'a> MessageRead<'a> for AndroidBuildProto {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(10) => msg.fingerprint = Some(r.read_string(bytes)?.to_owned()),
                Ok(18) => msg.hardware = Some(r.read_string(bytes)?.to_owned()),
                Ok(26) => msg.brand = Some(r.read_string(bytes)?.to_owned()),
                Ok(34) => msg.radio = Some(r.read_string(bytes)?.to_owned()),
                Ok(42) => msg.bootloader = Some(r.read_string(bytes)?.to_owned()),
                Ok(50) => msg.client_id = Some(r.read_string(bytes)?.to_owned()),
                Ok(56) => msg.time = Some(r.read_int64(bytes)?),
                Ok(64) => msg.package_version_code = Some(r.read_int32(bytes)?),
                Ok(74) => msg.device = Some(r.read_string(bytes)?.to_owned()),
                Ok(80) => msg.sdk_version = Some(r.read_int32(bytes)?),
                Ok(90) => msg.model = Some(r.read_string(bytes)?.to_owned()),
                Ok(98) => msg.manufacturer = Some(r.read_string(bytes)?.to_owned()),
                Ok(106) => msg.product = Some(r.read_string(bytes)?.to_owned()),
                Ok(112) => msg.ota_installed = Some(r.read_bool(bytes)?),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for AndroidBuildProto {
    fn get_size(&self) -> usize {
        0
        + self.fingerprint.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.hardware.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.brand.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.radio.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.bootloader.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.client_id.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.time.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.package_version_code.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.device.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.sdk_version.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.model.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.manufacturer.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.product.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.ota_installed.as_ref().map_or(0, |m| 1 + sizeof_varint(u64::from(*(m))))
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        if let Some(ref s) = self.fingerprint { w.write_with_tag(10, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.hardware { w.write_with_tag(18, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.brand { w.write_with_tag(26, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.radio { w.write_with_tag(34, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.bootloader { w.write_with_tag(42, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.client_id { w.write_with_tag(50, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.time { w.write_with_tag(56, |w| w.write_int64(*s))?; }
        if let Some(ref s) = self.package_version_code { w.write_with_tag(64, |w| w.write_int32(*s))?; }
        if let Some(ref s) = self.device { w.write_with_tag(74, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.sdk_version { w.write_with_tag(80, |w| w.write_int32(*s))?; }
        if let Some(ref s) = self.model { w.write_with_tag(90, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.manufacturer { w.write_with_tag(98, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.product { w.write_with_tag(106, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.ota_installed { w.write_with_tag(112, |w| w.write_bool(*s))?; }
        Ok(())
    }
}

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct AndroidCheckinEvent {
    pub tag: Option<String>,
    pub value: Option<String>,
    pub time_msec: Option<i64>,
}

impl<'a> MessageRead<'a> for AndroidCheckinEvent {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(10) => msg.tag = Some(r.read_string(bytes)?.to_owned()),
                Ok(18) => msg.value = Some(r.read_string(bytes)?.to_owned()),
                Ok(24) => msg.time_msec = Some(r.read_int64(bytes)?),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for AndroidCheckinEvent {
    fn get_size(&self) -> usize {
        0
        + self.tag.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.value.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.time_msec.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        if let Some(ref s) = self.tag { w.write_with_tag(10, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.value { w.write_with_tag(18, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.time_msec { w.write_with_tag(24, |w| w.write_int64(*s))?; }
        Ok(())
    }
}

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct AndroidCheckinProto {
    pub build: Option<AndroidBuildProto>,
    pub last_checkin_msec: Option<i64>,
    pub event: Vec<AndroidCheckinEvent>,
    pub cell_operator: Option<String>,
    pub sim_operator: Option<String>,
    pub roaming: Option<String>,
    pub user_number: Option<i32>,
    pub type_pb: DeviceType,
    pub chrome_build: Option<ChromeBuildProto>,
}

impl<'a> MessageRead<'a> for AndroidCheckinProto {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(10) => msg.build = Some(r.read_message::<AndroidBuildProto>(bytes)?),
                Ok(16) => msg.last_checkin_msec = Some(r.read_int64(bytes)?),
                Ok(26) => msg.event.push(r.read_message::<AndroidCheckinEvent>(bytes)?),
                Ok(50) => msg.cell_operator = Some(r.read_string(bytes)?.to_owned()),
                Ok(58) => msg.sim_operator = Some(r.read_string(bytes)?.to_owned()),
                Ok(66) => msg.roaming = Some(r.read_string(bytes)?.to_owned()),
                Ok(72) => msg.user_number = Some(r.read_int32(bytes)?),
                Ok(96) => msg.type_pb = r.read_enum(bytes)?,
                Ok(106) => msg.chrome_build = Some(r.read_message::<ChromeBuildProto>(bytes)?),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for AndroidCheckinProto {
    fn get_size(&self) -> usize {
        0
        + self.build.as_ref().map_or(0, |m| 1 + sizeof_len((m).get_size()))
        + self.last_checkin_msec.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.event.iter().map(|s| 1 + sizeof_len((s).get_size())).sum::<usize>()
        + self.cell_operator.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.sim_operator.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.roaming.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.user_number.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + if self.type_pb == DeviceType::DEVICE_ANDROID_OS { 0 } else { 1 + sizeof_varint(*(&self.type_pb) as u64) }
        + self.chrome_build.as_ref().map_or(0, |m| 1 + sizeof_len((m).get_size()))
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        if let Some(ref s) = self.build { w.write_with_tag(10, |w| w.write_message(s))?; }
        if let Some(ref s) = self.last_checkin_msec { w.write_with_tag(16, |w| w.write_int64(*s))?; }
        for s in &self.event { w.write_with_tag(26, |w| w.write_message(s))?; }
        if let Some(ref s) = self.cell_operator { w.write_with_tag(50, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.sim_operator { w.write_with_tag(58, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.roaming { w.write_with_tag(66, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.user_number { w.write_with_tag(72, |w| w.write_int32(*s))?; }
        if self.type_pb != DeviceType::DEVICE_ANDROID_OS { w.write_with_tag(96, |w| w.write_enum(*&self.type_pb as i32))?; }
        if let Some(ref s) = self.chrome_build { w.write_with_tag(106, |w| w.write_message(s))?; }
        Ok(())
    }
}

