# URL guard

This module provides the URL policy shared by outbound network tools. It
accepts HTTP and HTTPS URLs, applies an optional domain allowlist, rejects
local and non-global IP destinations, and checks DNS answers for rebinding.

## Public surface

- `validate_url` checks syntax, host policy, and local-address rules.
- `validate_url_with_dns_check` also resolves the host and returns a
  `ValidatedUrl` containing the vetted socket addresses.
- `normalize_allowed_domains`, `normalize_domain`, and
  `host_matches_allowlist` support host configuration.
- `is_private_or_local_host`, `is_non_global_v4`, and `is_non_global_v6`
  expose the address classification used by the guard.

## Security and operational constraints

Callers making outbound requests should use `validate_url_with_dns_check` and
pin the connection to every address in `ValidatedUrl::addrs`. Resolving the
hostname again in an HTTP client reopens the DNS-rebinding gap. Keep the URL's
hostname for TLS SNI and the Host header. Validate every redirect destination
before following it.

The guard rejects loopback, private, link-local, multicast, documentation,
shared-address, local names, IPv4-mapped and IPv4-compatible IPv6, private
IPv4 destinations embedded in NAT64 and 6to4 addresses, private Teredo server
and client addresses, and NAT64 translation prefixes.
Its lexical checks reject userinfo, backslashes, percent-encoded hosts, and
IPv6 URL literals because downstream URL parsers can interpret those forms
differently. This crate supplies no HTTP transport, so connection pinning and
redirect policy remain the caller's responsibility.
