# Gateway API Compatibility
## Summary
|Resource|Support Level|
|----|----|
|GatewayClass|Core Support|
|Gateway|Core Support|
|HttpRoute|Core Support|
|ReferenceGrant|Not Support|

## Expanding Gateway Api Impl
> Spacegate's unique implementation that is different or even opposite to the standard

### HttpRoute 
- metadata
  - annotations
    - priority (option) - default is 0
- spec
  - rules
    - backendRefs
      - kind - supports `Service`: k8s service  `ExternalService`: external service for k8s, backend name can be host or ip.

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