// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.

export type PluginInstanceId = { code: string, } & ({ "kind": "anon", uid: string, } | { "kind": "named", 
/**
 * name should be unique within the plugin code, composed of alphanumeric characters and hyphens
 */
name: string, } | { "kind": "mono" });