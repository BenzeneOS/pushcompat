//! UP endpoint validation. The bridge POSTs every message payload to this URL,
//! so an unvalidated endpoint turns the bridge into an SSRF proxy.

pub fn validate_endpoint(endpoint: &str, allowed_hosts: &[String]) -> Result<(), String> {
   let url = url::Url::parse(endpoint).map_err(|e| format!("invalid endpoint: {e}"))?;
   if url.scheme() != "https" {
      return Err("endpoint must be https".to_string());
   }
   if !url.username().is_empty() || url.password().is_some() {
      return Err("endpoint must not contain credentials".to_string());
   }
   let host = match url.host() {
      // IP literals are how link-local and metadata targets get addressed;
      // no real distributor needs them, so refuse rather than range-check.
      Some(url::Host::Domain(d)) => d.to_ascii_lowercase(),
      Some(_) => return Err("endpoint host must be a domain name".to_string()),
      None => return Err("endpoint must have a host".to_string()),
   };
   if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
      return Err("endpoint host not allowed".to_string());
   }
   if !allowed_hosts.is_empty() {
      let allowed = allowed_hosts.iter().any(|a| {
         let a = a.to_ascii_lowercase();
         host == a || host.ends_with(&format!(".{a}"))
      });
      if !allowed {
         return Err("endpoint host not in allowlist".to_string());
      }
   }
   Ok(())
}

#[cfg(test)]
mod tests {
   use super::*;

   #[test]
   fn endpoint_validation() {
      // Garbage, non-https, internal targets and embedded credentials.
      assert!(validate_endpoint("not a url", &[]).is_err());
      assert!(validate_endpoint("http://ntfy.example/t?up=1", &[]).is_err());
      assert!(validate_endpoint("https://127.0.0.1/t", &[]).is_err());
      assert!(validate_endpoint("https://[::1]/t", &[]).is_err());
      assert!(validate_endpoint("https://localhost/t", &[]).is_err());
      assert!(validate_endpoint("https://user:pw@ntfy.example/t", &[]).is_err());

      // No allowlist: any https domain is fine.
      assert!(validate_endpoint("https://ntfy.amaanq.com/upX?up=1", &[]).is_ok());

      // With one: host and subdomains, never suffix confusion.
      let allow = vec!["ntfy.amaanq.com".to_string()];
      assert!(validate_endpoint("https://ntfy.amaanq.com/t?up=1", &allow).is_ok());
      assert!(validate_endpoint("https://evil.example/t?up=1", &allow).is_err());
      assert!(validate_endpoint("https://ntfy.amaanq.com.evil.example/t", &allow).is_err());
   }
}
