import axios, { AxiosResponse, AxiosInstance, AxiosError } from 'axios'
import { BackendHost, Config, ConfigItem, PluginAttributes, SgGateway, SgHttpRoute } from '../model'
import { PluginConfig } from '../model/PluginConfig'
import { PluginInstanceId } from '../model/PluginInstanceId'
export * from 'axios'
const Client = {
    axiosInstance: axios as AxiosInstance,
    clientVersion: undefined as string | undefined,
}

export function getClient(): typeof Client {
    if (!(self as any).SpacegateAdminClient) {
        (self as any).SpacegateAdminClient = Client
    }
    return (self as any).SpacegateAdminClient as typeof Client
}

function pluginInstanceIdAsQuery(id: PluginInstanceId): URLSearchParams {
    let param = new URLSearchParams()
    for (const q of Object.keys(id)) {
        let val = (id as Record<string, any>)[q];
        if (typeof val === 'bigint') {
            param.set(q, val.toString())
        } else if (typeof val === 'string') {
            param.set(q, val)
        }
    }
    return param
}

export class ExceptionVersionConflict extends Error {
    constructor() {
        super('spacegate-admin-client: Client version conflict')
    }
}

export class ExceptionUnauthorized extends Error {
    constructor() {
        super('spacegate-admin-client: Unauthorized')
    }
}


export function setClient(...args: Parameters<typeof axios.create>) {
    let instance = axios.create(...args)
    instance.interceptors.request.use((cfg) => {
        cfg.headers['X-Client-Version'] = getClient().clientVersion ?? '0'
        return cfg
    });
    instance.interceptors.response.use(
        (resp) => {
            // this shall be lower case
            let value = resp.headers['x-server-version'];
            let is_conflict = (resp.status == 409)
            if (value !== undefined && value !== getClient().clientVersion) {
                if (is_conflict) {
                    throw new ExceptionVersionConflict()
                } else {
                    getClient().clientVersion = value
                }
            }
            return resp
        },
        (err) => {
            if (err instanceof AxiosError) {
                if (err.response?.status == 409) {
                    throw new ExceptionVersionConflict()
                }
                if (err.response?.status == 401) {
                    throw new ExceptionUnauthorized()
                }
            }
            throw err
        }
    )
    getClient().axiosInstance = instance
}


export async function getConfigItemGateway(
    gatewayName: string,
): Promise<AxiosResponse<SgGateway | null>> {
    return getClient().axiosInstance.get(`/config/item/${gatewayName}/gateway`)
}

export async function getConfigItemRoute(
    gateway_name: string,
    route_name: string,

): Promise<AxiosResponse<SgHttpRoute | null>> {
    return getClient().axiosInstance.get(`/config/item/${gateway_name}/route/item/${route_name}`)
}
export async function getConfigItemRouteNames(
    gatewayName: string,

): Promise<AxiosResponse<Array<string>>> {
    return getClient().axiosInstance.get(`/config/item/${gatewayName}/route/names`)
}
export async function getConfigItemAllRoutes(
    gatewayName: string,

): Promise<AxiosResponse<Record<string, SgHttpRoute>>> {
    return getClient().axiosInstance.get(`/config/item/${gatewayName}/route/all`)
}
export async function getConfigItem(gatewayName: string,): Promise<AxiosResponse<ConfigItem | null>> {
    return getClient().axiosInstance.get(`/config/item/${gatewayName}`)
}
export async function getConfigNames(): Promise<AxiosResponse<Array<string>>> {
    return getClient().axiosInstance.get(`/config/names`)
}
export async function getConfig(): Promise<AxiosResponse<Config>> {
    return getClient().axiosInstance.get(`/config`)
}
export async function getConfigPluginAll(): Promise<AxiosResponse<Array<PluginConfig>>> {
    return getClient().axiosInstance.get(`/config/plugin-all`)
}
export async function getConfigPlugin(id: PluginInstanceId): Promise<AxiosResponse<PluginConfig | null>> {
    const param = pluginInstanceIdAsQuery(id);
    return getClient().axiosInstance.get(`/config/plugin?${param}`)
}
export async function getConfigPluginsByCode(code: string): Promise<AxiosResponse<Array<PluginConfig>>> {
    return getClient().axiosInstance.get(`/config/plugins/${code}`)
}
/**********************************************
                       POST
**********************************************/
export async function postConfigItem(
    gatewayName: string,

    config_item: ConfigItem,
): Promise<AxiosResponse> {
    return getClient().axiosInstance.post(`/config/item/${gatewayName}`, config_item)
}
export async function postConfig(config: Config): Promise<AxiosResponse> {
    return getClient().axiosInstance.post(`/config`, config)
}
export async function postConfigItemGateway(
    gatewayName: string,

    gateway: SgGateway,
): Promise<AxiosResponse> {
    return getClient().axiosInstance.post(`/config/item/${gatewayName}/gateway`, gateway)
}
export async function postConfigItemRoute(
    gateway_name: string,
    route_name: string,

    route: SgHttpRoute,
): Promise<AxiosResponse> {
    return getClient().axiosInstance.post(`/config/item/${gateway_name}/route/item/${route_name}`, route)
}
export async function postConfigPlugin(config: PluginConfig): Promise<AxiosResponse> {
    const param = pluginInstanceIdAsQuery(config);
    return getClient().axiosInstance.post(`/config/plugin?${param}`, config.spec)
}
/**********************************************
                       PUT
**********************************************/
export async function putConfigItemGateway(
    gatewayName: string,

    gateway: SgGateway,
): Promise<AxiosResponse> {
    return getClient().axiosInstance.put(`/config/item/${gatewayName}/gateway`, gateway)
}

export async function putConfigItemRoute(
    gateway_name: string,
    route_name: string,

    route: SgHttpRoute,
): Promise<AxiosResponse> {
    return getClient().axiosInstance.put(`/config/item/${gateway_name}/route/item/${route_name}`, route)
}

export async function putConfigItem(
    gatewayName: string,

    config_item: ConfigItem,
): Promise<AxiosResponse> {
    return getClient().axiosInstance.put(`/config/item/${gatewayName}`, config_item)
}

export async function putConfig(config: Config): Promise<AxiosResponse> {
    return getClient().axiosInstance.put(`/config`, config)
}

export async function putConfigPlugin(config: PluginConfig): Promise<AxiosResponse> {
    const param = pluginInstanceIdAsQuery(config);
    return getClient().axiosInstance.put(`/config/plugin?${param}`, config.spec)
}
/**********************************************
                       DELETE
**********************************************/

export async function deleteConfigItemGateway(gatewayName: string,): Promise<AxiosResponse> {
    return getClient().axiosInstance.delete(`/config/item/${gatewayName}/gateway`)
}

export async function deleteConfigItemRoute(
    gateway_name: string,
    route_name: string,

): Promise<AxiosResponse> {
    return getClient().axiosInstance.delete(`/config/item/${gateway_name}/route/item/${route_name}`)
}

export async function deleteConfigItem(gatewayName: string,): Promise<AxiosResponse> {
    return getClient().axiosInstance.delete(`/config/item/${gatewayName}`)
}

export async function deleteConfigItemAllRoutes(gatewayName: string,): Promise<AxiosResponse> {
    return getClient().axiosInstance.delete(`/config/item/${gatewayName}/route/all`)
}

export async function deleteConfigPlugin(config: PluginConfig): Promise<AxiosResponse> {
    const param = pluginInstanceIdAsQuery(config);
    return getClient().axiosInstance.delete(`/config/plugin?${param}`)
}
/**********************************************
                        plugin
**********************************************/

export async function pluginList(): Promise<AxiosResponse<Array<string>>> {
    return getClient().axiosInstance.get(`/plugin/list`)
}

export async function pluginAttrAll(): Promise<AxiosResponse<Array<PluginAttributes>>> {
    return getClient().axiosInstance.get(`/plugin/attr-all`)
}

export async function pluginAttr(code: string): Promise<AxiosResponse<PluginAttributes | null>> {
    return getClient().axiosInstance.get(`/plugin/attr/${code}`)
}

export async function pluginSchema(code: string): Promise<AxiosResponse<unknown>> {
    return getClient().axiosInstance.get(`/plugin/schema/${code}`)
}

/**********************************************
                        auth
**********************************************/

export async function authLogin(ak: string, sk: string): Promise<AxiosResponse> {
    return getClient().axiosInstance.post(`/auth/login`, {
        ak, sk
    })
}

/**********************************************
                        instance
**********************************************/

export async function discoveryInstanceHealth(): Promise<AxiosResponse<Record<string, boolean>>> {
    return getClient().axiosInstance.get(`/discovery/instance/health`)
}

export async function discoveryInstanceList(): Promise<AxiosResponse<Array<string>>> {
    return getClient().axiosInstance.get(`/discovery/instance/list`)
}


export async function discoveryInstanceReloadGlobal(instance: string): Promise<AxiosResponse> {
    return getClient().axiosInstance.get(`/discovery/instance/reload/global?instance=${instance}`)
}

export async function discoveryInstanceReloadGateway(instance: string, gateway: string): Promise<AxiosResponse> {
    return getClient().axiosInstance.get(`/discovery/instance/reload/gateway?instance=${instance}&gateway=${gateway}`)
}

export async function discoveryInstanceReloadRoute(instance: string, gateway: string, route: string): Promise<AxiosResponse> {
    return getClient().axiosInstance.get(`/discovery/instance/reload/route?instance=${instance}&route=${route}&gateway=${gateway}`)
}

export async function discoveryBackends(): Promise<AxiosResponse<Array<BackendHost>>> {
    return getClient().axiosInstance.get(`/discovery/backends/`)
}