import axios, { AxiosResponse, AxiosInstance } from 'axios'
import { Config, ConfigItem, SgGateway, SgHttpRoute } from '../model'
export * from 'axios'
export const Client = {
    axiosInstance: axios as AxiosInstance,
    clientVersion: undefined as string | undefined,
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

export async function get_config_item_gateway(
    gatewayName: string,
): Promise<AxiosResponse<SgGateway | null>> {
    return Client.axiosInstance.get(`/config/item/${gatewayName}/gateway`)
}

export async function get_config_item_route(
    gateway_name: string,
    route_name: string,

): Promise<AxiosResponse<SgHttpRoute | null>> {
    return Client.axiosInstance.get(`/config/item/${gateway_name}/route/item/${route_name}`)
}
export async function get_config_item_route_names(
    gatewayName: string,

): Promise<AxiosResponse<Array<string>>> {
    return Client.axiosInstance.get(`/config/item/${gatewayName}/route/names`)
}
export async function get_config_item_all_routes(
    gatewayName: string,

): Promise<AxiosResponse<Record<string, SgHttpRoute>>> {
    return Client.axiosInstance.get(`/config/item/${gatewayName}/route/all`)
}
export async function get_config_item(gatewayName: string,): Promise<AxiosResponse<ConfigItem | null>> {
    return Client.axiosInstance.get(`/config/item/${gatewayName}`)
}
export async function get_config_names(): Promise<AxiosResponse<Array<string>>> {
    return Client.axiosInstance.get(`/config/names`)
}
export async function get_config(): Promise<AxiosResponse<Config>> {
    return Client.axiosInstance.get(`/config`)
}

/**********************************************
                       POST
**********************************************/
export async function post_config_item(
    gatewayName: string,

    config_item: ConfigItem,
): Promise<AxiosResponse> {
    return Client.axiosInstance.post(`/config/item/${gatewayName}`, config_item)
}
export async function post_config(config: Config): Promise<AxiosResponse> {
    return Client.axiosInstance.post(`/config`, config)
}
export async function post_config_item_gateway(
    gatewayName: string,

    gateway: SgGateway,
): Promise<AxiosResponse> {
    return Client.axiosInstance.post(`/config/item/${gatewayName}/gateway`, gateway)
}
export async function post_config_item_route(
    gateway_name: string,
    route_name: string,

    route: SgHttpRoute,
): Promise<AxiosResponse> {
    return Client.axiosInstance.post(`/config/item/${gateway_name}/route/item/${route_name}`, route)
}

/**********************************************
                       PUT
**********************************************/
export async function put_config_item_gateway(
    gatewayName: string,

    gateway: SgGateway,
): Promise<AxiosResponse> {
    return Client.axiosInstance.put(`/config/item/${gatewayName}/gateway`, gateway)
}

export async function put_config_item_route(
    gateway_name: string,
    route_name: string,

    route: SgHttpRoute,
): Promise<AxiosResponse> {
    return Client.axiosInstance.put(`/config/item/${gateway_name}/route/item/${route_name}`, route)
}

export async function put_config_item(
    gatewayName: string,

    config_item: ConfigItem,
): Promise<AxiosResponse> {
    return Client.axiosInstance.put(`/config/item/${gatewayName}`, config_item)
}

export async function put_config(config: Config): Promise<AxiosResponse> {
    return Client.axiosInstance.put(`/config`, config)
}

/**********************************************
                       DELETE
**********************************************/

export async function delete_config_item_gateway(gatewayName: string,): Promise<AxiosResponse> {
    return Client.axiosInstance.delete(`/config/item/${gatewayName}/gateway`)
}

export async function delete_config_item_route(
    gateway_name: string,
    route_name: string,

): Promise<AxiosResponse> {
    return Client.axiosInstance.delete(`/config/item/${gateway_name}/route/item/${route_name}`)
}

export async function delete_config_item(gatewayName: string,): Promise<AxiosResponse> {
    return Client.axiosInstance.delete(`/config/item/${gatewayName}`)
}

export async function delete_config_item_all_routes(gatewayName: string,): Promise<AxiosResponse> {
    return Client.axiosInstance.delete(`/config/item/${gatewayName}/route/all`)
}