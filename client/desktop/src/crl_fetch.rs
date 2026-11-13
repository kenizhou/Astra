// SPDX-License-Identifier: GPL-3.0-only
//! The host CRL fetcher (SP4c-3b D2): the desktop implementation of the
//! crypto store's [`CrlFetcher`] port. A bounded HTTP GET over the engine TLS
//! stack: a short timeout, a response-size cap, http(s) only, and no redirects.
//! The distribution-point URL is untrusted input (it comes from the cert), so
//! every bound here is a security boundary. A failure of any kind returns
//! `Err`, which the store's fetch-or-cache layer degrades to unchecked
//! revocation; the message open is never blocked.

use std::time::Duration;

use astra_crypto_store::{crl_metadata_of_der, CrlFetcher, FetchedCrl};

/// The total and connect budget for one CRL fetch. Short, so a slow or
/// hostile endpoint cannot stall the reading pipeline.
const CRL_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

/// The largest CRL body accepted. A CRL over the cap is refused (fail-soft),
/// so a malicious endpoint cannot exhaust memory with an endless stream.
const MAX_CRL_BYTES: usize = 5 * 1024 * 1024;

/// The bounded HTTP CRL fetcher. One shared client; construction realise the
/// engine TLS policy (ring provider, bundled + system roots) exactly as the
/// other desktop HTTP clients do.
#[derive(Debug)]
pub(crate) struct HttpCrlFetcher {
    client: reqwest::Client,
}

impl HttpCrlFetcher {
    /// Build the fetcher with the bounded client. Fails only when the TLS
    /// policy or HTTP client cannot be realised; the caller then falls back to
    /// the offline no-op fetcher (revocation stays cache-only).
    pub(crate) fn new() -> Result<Self, String> {
        let tls = engine_tls::client_config(&engine_tls::TlsPolicy::bundled_and_system())
            .map_err(|e| format!("CRL TLS config: {e}"))?;
        let client = tls
            .reqwest_builder()
            // No redirects: a distribution point that redirects degrades to
            // unchecked rather than chasing an untrusted hop chain.
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(CRL_FETCH_TIMEOUT)
            .timeout(CRL_FETCH_TIMEOUT)
            .build()
            .map_err(|e| format!("CRL HTTP client: {e}"))?;
        Ok(Self { client })
    }
}

#[async_trait::async_trait]
impl CrlFetcher for HttpCrlFetcher {
    async fn fetch(&self, crl_url: &str) -> Result<FetchedCrl, String> {
        let url = admissible_url(crl_url).ok_or_else(|| "CRL URL is not http(s)".to_string())?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("CRL fetch failed: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("CRL fetch status {}", response.status()));
        }
        let crl_der = read_capped(response).await?;
        // Parse the cache metadata in the smime crate (never here); a body
        // that is not a CRL is refused, degrading to unchecked.
        let meta = crl_metadata_of_der(&crl_der).map_err(|e| format!("CRL parse: {e}"))?;
        Ok(FetchedCrl {
            crl_der,
            issuer_dn: meta.issuer_dn,
            next_update: meta.next_update,
        })
    }
}

/// Parse `raw` and admit it only when the scheme is http or https. The URL is
/// attacker-influenced, so any other scheme (ldap, ftp, file, ...) is refused.
fn admissible_url(raw: &str) -> Option<url::Url> {
    let url = url::Url::parse(raw).ok()?;
    matches!(url.scheme(), "http" | "https").then_some(url)
}

/// Stream the response body up to [`MAX_CRL_BYTES`]; a body over the cap (or a
/// chunk read failure) is an error, never a partial CRL.
async fn read_capped(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("CRL read: {e}"))?
    {
        if body.len() + chunk.len() > MAX_CRL_BYTES {
            return Err("CRL response exceeds the size cap".to_string());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admissible_url_accepts_http_and_https() {
        assert!(admissible_url("http://ca.example.com/crl.der").is_some());
        assert!(admissible_url("https://ca.example.com/crl.der").is_some());
        // The scheme is case-insensitive (url::Url normalises it).
        assert!(admissible_url("HTTPS://ca.example.com/crl.der").is_some());
    }

    #[test]
    fn admissible_url_rejects_other_schemes_and_garbage() {
        for bad in [
            "ldap://ca.example.com/crl",
            "ftp://ca.example.com/crl",
            "file:///etc/crl",
            "not a url",
            "",
            "//ca.example.com/crl",
        ] {
            assert!(admissible_url(bad).is_none(), "must reject {bad:?}");
        }
    }
}
