use std::error;

#[derive(Debug)]
pub enum Error {
   /// Dependency failed, i.e. we blame them
   DependencyFailure(&'static str, &'static str),
   /// Dependency rejection, i.e. they blame us
   DependencyRejection(&'static str, String),
   /// Protobuf deserialization failure, probably a contract change
   ProtobufDecode(&'static str, quick_protobuf::Error),
   Request(&'static str, reqwest::Error),
   Response(&'static str, reqwest::Error),
   Socket(std::io::Error),
}

impl std::fmt::Display for Error {
   fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
      match self {
         Self::DependencyFailure(api, problem) => write!(f, "{api} API {problem}"),
         Self::DependencyRejection(api, reason) => {
            write!(f, "{api} API rejected request: {reason}")
         },
         Self::ProtobufDecode(kind, e) => write!(f, "Error decoding {kind}: {e}"),
         Self::Request(kind, e) => write!(f, "{kind} API request error: {e}"),
         Self::Response(kind, e) => write!(f, "{kind} API response error: {e}"),
         Self::Socket(e) => write!(f, "TCP error: {e}"),
      }
   }
}

impl std::error::Error for Error {
   fn source(&self) -> Option<&(dyn error::Error + 'static)> {
      match *self {
         Self::DependencyFailure(..) => None,
         Self::DependencyRejection(..) => None,
         Self::ProtobufDecode(_, ref e) => Some(e),
         Self::Request(_, ref e) => Some(e),
         Self::Response(_, ref e) => Some(e),
         Self::Socket(ref e) => Some(e),
      }
   }
}
