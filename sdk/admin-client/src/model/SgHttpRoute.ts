// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.
import type { PluginInstanceId } from "./PluginInstanceId";
import type { SgHttpRouteRule } from "./SgHttpRouteRule";

export type SgHttpRoute<P = PluginInstanceId> = { 
/**
 * Route name
 */
route_name: string, 
/**
 * Hostnames defines a set of hostname that should match against the HTTP Host header to select a HTTPRoute to process the request.
 */
hostnames: Array<string> | null, 
/**
 * Filters define the filters that are applied to requests that match this hostnames.
 */
plugins: Array<P>, 
/**
 * Rules are a list of HTTP matchers, filters and actions.
 */
rules: Array<SgHttpRouteRule<P>>, 
/**
 * Rule priority, the rule of higher priority will be chosen.
 */
priority: number, };