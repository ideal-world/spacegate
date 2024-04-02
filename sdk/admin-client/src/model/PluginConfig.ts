// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.
import type { JsonValue } from "./serde_json/JsonValue";

export type PluginConfig = { spec: JsonValue, code: string, } & ({ uid: bigint, } | { 
/**
 * name should be unique within the plugin code, composed of alphanumeric characters and hyphens
 */
name: string, } | Record<string, never>);