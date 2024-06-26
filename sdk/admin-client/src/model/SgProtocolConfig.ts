// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.
import type { SgTlsConfig } from "./SgTlsConfig";

export type SgProtocolConfig = { "type": "http" } | { "type": "https", 
/**
 * TLS is the TLS configuration for the Listener.
 * This field is required if the Protocol field is “HTTPS” or “TLS”. It is invalid to set this field if the Protocol field is “HTTP”, “TCP”, or “UDP”.
 */
tls: SgTlsConfig, };