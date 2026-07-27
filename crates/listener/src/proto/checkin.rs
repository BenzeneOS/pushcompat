// Automatically generated rust module for 'checkin.proto' file
// Regenerate from the repository root: nix develop -c bash nix/regen-proto.sh

#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(unused_imports)]
#![allow(unknown_lints)]
#![allow(clippy::all)]
#![cfg_attr(rustfmt, rustfmt_skip)]


use quick_protobuf::{MessageInfo, MessageRead, MessageWrite, BytesReader, Writer, WriterBackend, Result};
use quick_protobuf::sizeofs::{sizeof_len, sizeof_varint};
use super::*;

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct GservicesSetting {
    pub name: Vec<u8>,
    pub value: Vec<u8>,
}

impl<'a> MessageRead<'a> for GservicesSetting {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(10) => msg.name = r.read_bytes(bytes)?.to_owned(),
                Ok(18) => msg.value = r.read_bytes(bytes)?.to_owned(),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for GservicesSetting {
    fn get_size(&self) -> usize {
        0
        + 1 + sizeof_len((&self.name).len())
        + 1 + sizeof_len((&self.value).len())
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        w.write_with_tag(10, |w| w.write_bytes(&**&self.name))?;
        w.write_with_tag(18, |w| w.write_bytes(&**&self.value))?;
        Ok(())
    }
}

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct AndroidCheckinRequest {
    pub imei: Option<String>,
    pub meid: Option<String>,
    pub mac_addr: Vec<String>,
    pub mac_addr_type: Vec<String>,
    pub serial_number: Option<String>,
    pub esn: Option<String>,
    pub id: Option<i64>,
    pub logging_id: Option<i64>,
    pub digest: Option<String>,
    pub locale: Option<String>,
    pub checkin: super::android_checkin::AndroidCheckinProto,
    pub desired_build: Option<String>,
    pub market_checkin: Option<String>,
    pub account_cookie: Vec<String>,
    pub time_zone: Option<String>,
    pub security_token: Option<u64>,
    pub version: Option<i32>,
    pub ota_cert: Vec<String>,
    pub fragment: Option<i32>,
    pub user_name: Option<String>,
    pub user_serial_number: Option<i32>,
}

impl<'a> MessageRead<'a> for AndroidCheckinRequest {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(10) => msg.imei = Some(r.read_string(bytes)?.to_owned()),
                Ok(82) => msg.meid = Some(r.read_string(bytes)?.to_owned()),
                Ok(74) => msg.mac_addr.push(r.read_string(bytes)?.to_owned()),
                Ok(154) => msg.mac_addr_type.push(r.read_string(bytes)?.to_owned()),
                Ok(130) => msg.serial_number = Some(r.read_string(bytes)?.to_owned()),
                Ok(138) => msg.esn = Some(r.read_string(bytes)?.to_owned()),
                Ok(16) => msg.id = Some(r.read_int64(bytes)?),
                Ok(56) => msg.logging_id = Some(r.read_int64(bytes)?),
                Ok(26) => msg.digest = Some(r.read_string(bytes)?.to_owned()),
                Ok(50) => msg.locale = Some(r.read_string(bytes)?.to_owned()),
                Ok(34) => msg.checkin = r.read_message::<super::android_checkin::AndroidCheckinProto>(bytes)?,
                Ok(42) => msg.desired_build = Some(r.read_string(bytes)?.to_owned()),
                Ok(66) => msg.market_checkin = Some(r.read_string(bytes)?.to_owned()),
                Ok(90) => msg.account_cookie.push(r.read_string(bytes)?.to_owned()),
                Ok(98) => msg.time_zone = Some(r.read_string(bytes)?.to_owned()),
                Ok(105) => msg.security_token = Some(r.read_fixed64(bytes)?),
                Ok(112) => msg.version = Some(r.read_int32(bytes)?),
                Ok(122) => msg.ota_cert.push(r.read_string(bytes)?.to_owned()),
                Ok(160) => msg.fragment = Some(r.read_int32(bytes)?),
                Ok(170) => msg.user_name = Some(r.read_string(bytes)?.to_owned()),
                Ok(176) => msg.user_serial_number = Some(r.read_int32(bytes)?),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for AndroidCheckinRequest {
    fn get_size(&self) -> usize {
        0
        + self.imei.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.meid.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.mac_addr.iter().map(|s| 1 + sizeof_len((s).len())).sum::<usize>()
        + self.mac_addr_type.iter().map(|s| 2 + sizeof_len((s).len())).sum::<usize>()
        + self.serial_number.as_ref().map_or(0, |m| 2 + sizeof_len((m).len()))
        + self.esn.as_ref().map_or(0, |m| 2 + sizeof_len((m).len()))
        + self.id.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.logging_id.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.digest.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.locale.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + 1 + sizeof_len((&self.checkin).get_size())
        + self.desired_build.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.market_checkin.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.account_cookie.iter().map(|s| 1 + sizeof_len((s).len())).sum::<usize>()
        + self.time_zone.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.security_token.as_ref().map_or(0, |_| 1 + 8)
        + self.version.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.ota_cert.iter().map(|s| 1 + sizeof_len((s).len())).sum::<usize>()
        + self.fragment.as_ref().map_or(0, |m| 2 + sizeof_varint(*(m) as u64))
        + self.user_name.as_ref().map_or(0, |m| 2 + sizeof_len((m).len()))
        + self.user_serial_number.as_ref().map_or(0, |m| 2 + sizeof_varint(*(m) as u64))
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        if let Some(ref s) = self.imei { w.write_with_tag(10, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.meid { w.write_with_tag(82, |w| w.write_string(&**s))?; }
        for s in &self.mac_addr { w.write_with_tag(74, |w| w.write_string(&**s))?; }
        for s in &self.mac_addr_type { w.write_with_tag(154, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.serial_number { w.write_with_tag(130, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.esn { w.write_with_tag(138, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.id { w.write_with_tag(16, |w| w.write_int64(*s))?; }
        if let Some(ref s) = self.logging_id { w.write_with_tag(56, |w| w.write_int64(*s))?; }
        if let Some(ref s) = self.digest { w.write_with_tag(26, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.locale { w.write_with_tag(50, |w| w.write_string(&**s))?; }
        w.write_with_tag(34, |w| w.write_message(&self.checkin))?;
        if let Some(ref s) = self.desired_build { w.write_with_tag(42, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.market_checkin { w.write_with_tag(66, |w| w.write_string(&**s))?; }
        for s in &self.account_cookie { w.write_with_tag(90, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.time_zone { w.write_with_tag(98, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.security_token { w.write_with_tag(105, |w| w.write_fixed64(*s))?; }
        if let Some(ref s) = self.version { w.write_with_tag(112, |w| w.write_int32(*s))?; }
        for s in &self.ota_cert { w.write_with_tag(122, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.fragment { w.write_with_tag(160, |w| w.write_int32(*s))?; }
        if let Some(ref s) = self.user_name { w.write_with_tag(170, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.user_serial_number { w.write_with_tag(176, |w| w.write_int32(*s))?; }
        Ok(())
    }
}

#[expect(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct AndroidCheckinResponse {
    pub stats_ok: bool,
    pub time_msec: Option<i64>,
    pub digest: Option<String>,
    pub settings_diff: Option<bool>,
    pub delete_setting: Vec<String>,
    pub setting: Vec<GservicesSetting>,
    pub market_ok: Option<bool>,
    pub android_id: Option<u64>,
    pub security_token: Option<u64>,
    pub version_info: Option<String>,
}

impl<'a> MessageRead<'a> for AndroidCheckinResponse {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(8) => msg.stats_ok = r.read_bool(bytes)?,
                Ok(24) => msg.time_msec = Some(r.read_int64(bytes)?),
                Ok(34) => msg.digest = Some(r.read_string(bytes)?.to_owned()),
                Ok(72) => msg.settings_diff = Some(r.read_bool(bytes)?),
                Ok(82) => msg.delete_setting.push(r.read_string(bytes)?.to_owned()),
                Ok(42) => msg.setting.push(r.read_message::<GservicesSetting>(bytes)?),
                Ok(48) => msg.market_ok = Some(r.read_bool(bytes)?),
                Ok(57) => msg.android_id = Some(r.read_fixed64(bytes)?),
                Ok(65) => msg.security_token = Some(r.read_fixed64(bytes)?),
                Ok(90) => msg.version_info = Some(r.read_string(bytes)?.to_owned()),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for AndroidCheckinResponse {
    fn get_size(&self) -> usize {
        0
        + 1 + sizeof_varint(u64::from(*(&self.stats_ok)))
        + self.time_msec.as_ref().map_or(0, |m| 1 + sizeof_varint(*(m) as u64))
        + self.digest.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
        + self.settings_diff.as_ref().map_or(0, |m| 1 + sizeof_varint(u64::from(*(m))))
        + self.delete_setting.iter().map(|s| 1 + sizeof_len((s).len())).sum::<usize>()
        + self.setting.iter().map(|s| 1 + sizeof_len((s).get_size())).sum::<usize>()
        + self.market_ok.as_ref().map_or(0, |m| 1 + sizeof_varint(u64::from(*(m))))
        + self.android_id.as_ref().map_or(0, |_| 1 + 8)
        + self.security_token.as_ref().map_or(0, |_| 1 + 8)
        + self.version_info.as_ref().map_or(0, |m| 1 + sizeof_len((m).len()))
    }

    fn write_message<W>(&self, w: &mut Writer<W>) -> Result<()> where W: WriterBackend {
        w.write_with_tag(8, |w| w.write_bool(*&self.stats_ok))?;
        if let Some(ref s) = self.time_msec { w.write_with_tag(24, |w| w.write_int64(*s))?; }
        if let Some(ref s) = self.digest { w.write_with_tag(34, |w| w.write_string(&**s))?; }
        if let Some(ref s) = self.settings_diff { w.write_with_tag(72, |w| w.write_bool(*s))?; }
        for s in &self.delete_setting { w.write_with_tag(82, |w| w.write_string(&**s))?; }
        for s in &self.setting { w.write_with_tag(42, |w| w.write_message(s))?; }
        if let Some(ref s) = self.market_ok { w.write_with_tag(48, |w| w.write_bool(*s))?; }
        if let Some(ref s) = self.android_id { w.write_with_tag(57, |w| w.write_fixed64(*s))?; }
        if let Some(ref s) = self.security_token { w.write_with_tag(65, |w| w.write_fixed64(*s))?; }
        if let Some(ref s) = self.version_info { w.write_with_tag(90, |w| w.write_string(&**s))?; }
        Ok(())
    }
}

