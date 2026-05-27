# Gateway API Compatibility

## Summary

| Resource                            | Core Support Level  | Extended Support Level | Implementation-Specific Support Level | API Version |
|-------------------------------------|---------------------|------------------------|---------------------------------------|-------------|
| GatewayClass                        | Support             | Not supported          | Not supported                         | v1beta1     |
| [Gateway](#gateway)                 | Partially Supported | Not supported          | Not supported                         | v1beta1     |
| [HTTPRoute](#httproute)             | Partially Supported | Partially Supported    | Partially Supported                   | v1beta1     |
| [ReferenceGrant](#referencegrant)   | Not Support         | Not Support            | Not supported                         | v1beta1     |
| [Custom policies](#custom-policies) | Not supported       | N/A                    | Not supported                         | N/A         |
| [TLSRoute](#tlsroute)               | Not supported       | Not supported          | Not supported                         | N/A         |
| [TCPRoute](#tcproute)               | Not supported       | Not supported          | Not supported                         | N/A         |
| [UDPRoute](#udproute)               | Not supported       | Not supported          | Not supported                         | N/A         |

## Gateway Api Resources

For a description of each field, visit the [Gateway API documentation](https://gateway-api.sigs.k8s.io/references/spec/).

### Gateway

> Support Levels:
>
> - Core: Partially Supported.
> - Extended: Not supported.
> - Implementation-specific: Not supported.

Fields:

* `spec`
    * `gatewayClassName` - supported.
    * `listeners`
        * `name` - supported.
        * `hostname` - supported.
        * `port` - supported.
        * `protocol` - partially supported. Allowed values: `HTTP`, `HTTPS` ,`WS`.
        * `tls`
            * `mode` - partially supported. Allowed value: `Terminate`.
            * `certificateRefs` - The TLS certificate and key must be stored in a Secret resource of
              type `kubernetes.io/tls`. Only a single reference is supported.
            * `options` - not supported.
        * `allowedRoutes` - not supported.
    * `addresses` - not supported.
* `status` - not supported.

### HTTPRoute

> Support Levels:
>
> - Core: Partially Supported.
> - Extended: Partially supported.
> - Implementation-specific: Partially supported.
    >   > Fields:

* `spec`
    * `parentRefs` - partially supported. Kind only values `Gateway`.
    * `hostnames` - supported.
    * `rules`
        * `matches`
            * `path` - supported. Allowed: `PathPrefix` , `Exact` , `RegularExpression`.
            * `headers` - supported. Allowed: `Exact` , `RegularExpression`.
            * `queryParams` - supported. Allowed: `Exact` , `RegularExpression`.
            * `method` - supported.
        * `filters`
            * `type` - supported.
            * `requestRedirect` - supported .
            * `requestHeaderModifier` - supported.
            * `responseHeaderModifier` - supported.
            * `urlRewrite` - supported.
            * `requestMirror`, `extensionRef` - not supported.
        * `backendRefs` - supported.
            * `filters` - same as rules.filters support;
* `status` - not supported.

### ReferenceGrant

> Support Levels:
>
> - Core: Not Supported.
    >   > Fields:

* `spec` - Not Supported.
    * `to`
        * `group`
        * `kind`
        * `name`
    * `from`
        * `group`
        * `kind`
        * `namespace`

### TLSRoute

> Status: Not supported.

### TCPRoute

> Status: Not supported.

### UDPRoute

> Status: Not supported.

### Custom Policies

> Status: Not supported.

## Expanding Gateway Api Impl

> Spacegate's unique implementation that is different or even opposite to the standard

### Gateway
- metadata
    - annotations
        - log_level (option) - spacegate log level : see [rust log level](https://docs.rs/log/latest/log/enum.Level.html)
        - redis_url (option) - spacegate redis url
        - lang (option) - spacegate i8n support
        - ignore_tls_verification (option) - ignore backend tls verification
### HttpRoute

- metadata
    - annotations
        - priority (option) - default is 0
- spec
    - rules
        - backendRefs
            - kind - supports `Service`: k8s service
              `External`: external-k8s service, backend name can be host or ip.
              `ExternalHttp`: external-k8s http service, backend name can be host or ip.
              `ExternalHttps`: external https service for k8s, similar to `ExternalHttp`.

### SgFilter

> spacegate's CRD,used to express the attachment of a specified filter to a resource

- spec
    - filters
        - code
        - name (option)
        - enable (option)
        - config - json Value
    - targetRefs:
        - kind - `Gateway` `HTTPRoute`
        - namespace (option)
        - name

### Higress-compatible WasmPlugin

Spacegate can read Higress-style `extensions.higress.io/v1alpha1` `WasmPlugin` resources and translate them into the internal `code = "wasm"` plugin runtime configuration.

Supported fields:

- `spec.url` - local path, `file://`, `http://`, `https://`, or OCI wasm image URL (`oci://`, `docker://`, `image://`).
- `spec.sha256` - optional wasm byte digest, plain hex or `sha256:<hex>`.
- `spec.pluginName` - exposed to the proxy-wasm guest as `plugin_name`.
- `spec.defaultConfig` - converted to a Spacegate wasm plugin instance and mounted at Gateway level.
- `spec.defaultConfigDisable` - disables the generated Gateway-level default plugin instance.
- `spec.matchRules[].ingress` - generates per-rule wasm plugin instances and mounts them on matching Spacegate routes.
- `spec.matchRules[].domain` - generates per-rule wasm plugin instances and mounts them on routes whose hostnames match.
- `spec.matchRules[].service` - generates per-rule wasm plugin instances and mounts them on matching backends.
- `spec.matchRules[].config/configDisable` - configures or disables each generated rule-level plugin instance.
- `spec.failStrategy` - accepts `FAIL_OPEN`/`FAIL_CLOSE` and maps to Spacegate `fail_open`/`fail_close`.
- `spec.phase` - participates in plugin ordering (`AUTHN` before `AUTHZ` before unspecified before `STATS`).
- `spec.priority` - used inside the same phase; higher priority plugins are mounted earlier.
- `spec.imagePullPolicy` - `Always` disables the Spacegate module cache for that plugin instance.
- `spec.imagePullSecret` - for OCI URLs, Spacegate reads Docker config (`.dockerconfigjson`/`.dockercfg`) or basic-auth (`username`/`password`) Kubernetes Secrets and passes the registry credentials to the wasm runtime.
- `status` - Spacegate writes `observedGeneration`, `phase`, `digest`, and `message` during K8s watch reconciliation.

Current limitations:

- OCI layer selection supports wasm media types (`application/vnd.module.wasm.content.layer.v1+wasm`, `application/vnd.wasm.content.layer.v1+wasm`, `application/wasm`) and falls back to a single-layer artifact.
- `phase` currently maps to ordering only, not to separate Spacegate execution pipelines.
