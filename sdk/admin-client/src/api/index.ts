import axios, { AxiosResponse, AxiosInstance } from 'axios'
import { Config, ConfigItem, PluginAttributes, SgGateway, SgHttpRoute } from '../model'
import { PluginConfig } from '../model/PluginConfig'
import { PluginInstanceName } from '../model/PluginInstanceName'
import { PluginInstanceId } from '../model/PluginInstanceId'
export * from 'axios'
export const Client = {
    axiosInstance: axios as AxiosInstance,
    clientVersion: undefined as string | undefined,
}

function pluginInstanceIdAsQuery(id: PluginInstanceId): URLSearchParams {
    let param = new URLSearchParams()
    for (const q in Object.keys(id)) {
        if (q === 'code' || q === 'uid' || q === 'name') {
            let val = (id as Record<string, any>)[q];
            if (typeof val === 'bigint') {
                param.set(q, val.toString())
            } else if (typeof val === 'string') {
                param.set(q, val)
            }
        }
    }
    return param
}

export class ExceptionVersionConflict extends Error {
    constructor() {
        super('spacegate-admin-client: Client version conflict')
    }
}

export function setClient(...args: Parameters<typeof axios.create>) {
    let instance = axios.create(...args)
    instance.interceptors.request.use((cfg) => {
        cfg.headers['X-Client-Version'] = Client.clientVersion ?? '0'
        return cfg
    });
    instance.interceptors.response.use((resp) => {
        // this shall be lower case
        let value = resp.headers['x-server-version'];
        let is_conflict = (resp.status == 409)
        if (value !== undefined && value !== Client.clientVersion) {
            if (is_conflict) {
                throw new ExceptionVersionConflict()
            } else {
                Client.clientVersion = value
            }
        }
        return resp
    })
    Client.axiosInstance = instance
}


export async function getConfigItemGateway(
    gatewayName: string,
): Promise<AxiosResponse<SgGateway | null>> {
    return Client.axiosInstance.get(`/config/item/${gatewayName}/gateway`)
}

export async function getConfigItemRoute(
    gateway_name: string,
    route_name: string,

): Promise<AxiosResponse<SgHttpRoute | null>> {
    return Client.axiosInstance.get(`/config/item/${gateway_name}/route/item/${route_name}`)
}
export async function getConfigItemRouteNames(
    gatewayName: string,

): Promise<AxiosResponse<Array<string>>> {
    return Client.axiosInstance.get(`/config/item/${gatewayName}/route/names`)
}
export async function getConfigItemAllRoutes(
    gatewayName: string,

): Promise<AxiosResponse<Record<string, SgHttpRoute>>> {
    return Client.axiosInstance.get(`/config/item/${gatewayName}/route/all`)
}
export async function getConfigItem(gatewayName: string,): Promise<AxiosResponse<ConfigItem | null>> {
    return Client.axiosInstance.get(`/config/item/${gatewayName}`)
}
export async function getConfigNames(): Promise<AxiosResponse<Array<string>>> {
    return Client.axiosInstance.get(`/config/names`)
}
export async function getConfig(): Promise<AxiosResponse<Config>> {
    return Client.axiosInstance.get(`/config`)
}
export async function getConfigPluginAll(): Promise<AxiosResponse<Array<PluginConfig>>> {
    return Client.axiosInstance.get(`/config/plugin-all`)
}
export async function getConfigPlugin(id: PluginInstanceId): Promise<AxiosResponse<PluginConfig | null>> {
    const param = pluginInstanceIdAsQuery(id);
    return Client.axiosInstance.get(`/config/plugin?${param}`)
}
export async function getConfigPluginsByCode(code: string): Promise<AxiosResponse<Array<PluginConfig>>> {
    return Client.axiosInstance.get(`/config/plugins/${code}`)
}
/**********************************************
                       POST
**********************************************/
export async function postConfigItem(
    gatewayName: string,

    config_item: ConfigItem,
): Promise<AxiosResponse> {
    return Client.axiosInstance.post(`/config/item/${gatewayName}`, config_item)
}
export async function postConfig(config: Config): Promise<AxiosResponse> {
    return Client.axiosInstance.post(`/config`, config)
}
export async function postConfigItemGateway(
    gatewayName: string,

    gateway: SgGateway,
): Promise<AxiosResponse> {
    return Client.axiosInstance.post(`/config/item/${gatewayName}/gateway`, gateway)
}
export async function postConfigItemRoute(
    gateway_name: string,
    route_name: string,

    route: SgHttpRoute,
): Promise<AxiosResponse> {
    return Client.axiosInstance.post(`/config/item/${gateway_name}/route/item/${route_name}`, route)
}
export async function postConfigPlugin(config: PluginConfig): Promise<AxiosResponse> {
    const param = pluginInstanceIdAsQuery(config);
    return Client.axiosInstance.post(`/config/plugin?${param}`, config.spec)
}
/**********************************************
                       PUT
**********************************************/
export async function putConfigItemGateway(
    gatewayName: string,

    gateway: SgGateway,
): Promise<AxiosResponse> {
    return Client.axiosInstance.put(`/config/item/${gatewayName}/gateway`, gateway)
}

export async function putConfigItemRoute(
    gateway_name: string,
    route_name: string,

    route: SgHttpRoute,
): Promise<AxiosResponse> {
    return Client.axiosInstance.put(`/config/item/${gateway_name}/route/item/${route_name}`, route)
}

export async function putConfigItem(
    gatewayName: string,

    config_item: ConfigItem,
): Promise<AxiosResponse> {
    return Client.axiosInstance.put(`/config/item/${gatewayName}`, config_item)
}

export async function putConfig(config: Config): Promise<AxiosResponse> {
    return Client.axiosInstance.put(`/config`, config)
}

export async function putConfigPlugin(config: PluginConfig): Promise<AxiosResponse> {
    const param = pluginInstanceIdAsQuery(config);
    return Client.axiosInstance.put(`/config/plugin?${param}`, config.spec)
}
/**********************************************
                       DELETE
**********************************************/

export async function deleteConfigItemGateway(gatewayName: string,): Promise<AxiosResponse> {
    return Client.axiosInstance.delete(`/config/item/${gatewayName}/gateway`)
}

export async function deleteConfigItemRoute(
    gateway_name: string,
    route_name: string,

): Promise<AxiosResponse> {
    return Client.axiosInstance.delete(`/config/item/${gateway_name}/route/item/${route_name}`)
}

export async function deleteConfigItem(gatewayName: string,): Promise<AxiosResponse> {
    return Client.axiosInstance.delete(`/config/item/${gatewayName}`)
}

export async function deleteConfigItemAllRoutes(gatewayName: string,): Promise<AxiosResponse> {
    return Client.axiosInstance.delete(`/config/item/${gatewayName}/route/all`)
}

export async function deleteConfigPlugin(config: PluginConfig): Promise<AxiosResponse> {
    const param = pluginInstanceIdAsQuery(config);
    return Client.axiosInstance.delete(`/config/plugin?${param}`)
}
/**********************************************
                        plugin
**********************************************/

export async function pluginList(): Promise<AxiosResponse<Array<string>>> {
    return Client.axiosInstance.get(`/plugin/list`)
}

export async function pluginAttrAll(): Promise<AxiosResponse<Array<PluginAttributes>>> {
    return Client.axiosInstance.get(`/plugin/attr-all`)
}

export async function pluginAttr(code: string): Promise<AxiosResponse<PluginAttributes | null>> {
    return Client.axiosInstance.get(`/plugin/attr/${code}`)
}

export async function pluginSchema(code: string): Promise<AxiosResponse<unknown>> {
    return Client.axiosInstance.get(`/plugin/schema/${code}`)
}