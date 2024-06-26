// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.
import type { PluginInstanceId } from "./PluginInstanceId";
import type { SgBackendRef } from "./SgBackendRef";
import type { SgHttpRouteMatch } from "./SgHttpRouteMatch";

export type SgHttpRouteRule<P = PluginInstanceId> = { 
/**
 * Matches define conditions used for matching the rule against incoming HTTP requests. Each match is independent, i.e. this rule will be matched if any one of the matches is satisfied.
 */
matches: Array<SgHttpRouteMatch> | null, 
/**
 * Filters define the filters that are applied to requests that match this rule.
 */
plugins: Array<P>, 
/**
 * BackendRefs defines the backend(s) where matching requests should be sent.
 */
backends: Array<SgBackendRef<P>>, 
/**
 * Timeout define the timeout for requests that match this rule.
 */
timeout_ms: number | null, };