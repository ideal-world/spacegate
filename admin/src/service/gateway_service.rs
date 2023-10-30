#[cfg(feature = "k8s")]
use crate::helper::get_k8s_client;
use crate::model::query_dto::{GatewayQueryDto, ToInstance};
#[cfg(feature = "k8s")]
use crate::model::ToFields;
use std::clone;
use std::process::id;

use crate::model::vo::gateway_vo::SgGatewayVO;
use crate::model::vo::Vo;
use crate::service::plugin_service::PluginServiceVo;
#[cfg(feature = "k8s")]
use k8s_gateway_api::Gateway;
#[cfg(feature = "k8s")]
use k8s_openapi::api::core::v1::Secret;
#[cfg(feature = "k8s")]
use k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference;
#[cfg(feature = "k8s")]
use kernel_common::constants::DEFAULT_NAMESPACE;
use kernel_common::inner_model::gateway::{SgGateway, SgListener, SgParameters, SgProtocol, SgTlsConfig, SgTlsMode};
#[cfg(feature = "k8s")]
use kube::api::{DeleteParams, PostParams};

#[cfg(feature = "k8s")]
use kube::{api::ListParams, Api, ResourceExt};

use crate::model::vo_converter::VoConv;
use kernel_common::helper::k8s_helper::{parse_k8s_obj_unique, WarpKubeResult};
use tardis::basic::error::TardisError;
use tardis::basic::result::TardisResult;

use super::base_service::VoBaseService;

pub struct GatewayServiceVo;

impl VoBaseService<SgGatewayVO> for GatewayServiceVo {}

impl GatewayServiceVo {
    pub async fn list(query: GatewayQueryDto) -> TardisResult<Vec<SgGatewayVO>> {
        let query = query.to_instance()?;
        Ok(Self::get_type_map().await?.into_values().into_iter().filter(|g| if let Some(q_name) = &query.name { q_name.is_match(&g.name) } else { true }
            &&if let Some(q_port) = &query.port { g.listeners.iter().any(|l| l.port.eq( q_port)) } else { true }&&
            if let Some(q_hostname) = &query.hostname {
                g.listeners.iter().any(|l| if let Some(l_hostname)=&l.hostname{q_hostname.is_match(l_hostname)}else { false })
            } else { true })
            .collect())
    }
    pub async fn add(add: SgGatewayVO) -> TardisResult<SgGatewayVO> {
        #[cfg(feature = "k8s")]
        {
            let (namespace, _) = parse_k8s_obj_unique(&add.get_unique_name());
            let (gateway, secrets, sgfilters) = add.clone().to_model().await?.to_kube_gateway();

            let (gateway_api, secret_api): (Api<Gateway>, Api<Secret>) =
                (Api::namespaced(get_k8s_client().await?, &namespace), Api::namespaced(get_k8s_client().await?, &namespace));

            let result_gateway = gateway_api.create(&PostParams::default(), &gateway).await.map_err(|e| TardisError::io_error(&format!("[SG.admin] error:{e}"), ""))?;

            for mut s in secrets {
                s.metadata.owner_references = Some(vec![OwnerReference {
                    api_version: "gateway.networking.k8s.io/v1beta1".to_string(),
                    kind: "Gateway".to_string(),
                    name: result_gateway.name_any(),
                    uid: result_gateway.uid().expect("Can not get create gateway uid"),
                    ..Default::default()
                }]);
                secret_api.create(&PostParams::default(), &s).await.map_err(|e| TardisError::io_error(&format!("[SG.admin] error:{e}"), ""))?;
            }

            PluginServiceVo::add_sgfilter_vec(sgfilters).await?;
        }
        Self::add_vo(add).await
    }

    pub async fn update_by_id(id: &str) -> TardisResult<()> {
        let gateway_o = Self::get_by_id(&id).await?;
        GatewayServiceVo::update(gateway_o).await
    }

    pub async fn update(update: SgGatewayVO) -> TardisResult<()> {
        let gateway_o = Self::get_by_id(&update.get_unique_name()).await?;
        //todo
        Ok(())
    }

    #[cfg(feature = "k8s")]
    pub async fn delete(id: &str) -> TardisResult<()> {
        let (namespace, name) = parse_k8s_obj_unique(id);
        let gateway_api: Api<Gateway> = Self::get_gateway_api(&Some(namespace)).await?;

        gateway_api.delete(&name, &DeleteParams::default()).await.warp_result_by_method("Delete Gateway")?;

        Self::delete_vo(id).await?;

        Ok(())
    }

    #[cfg(feature = "k8s")]
    async fn get_gateway_api(namespace: &Option<String>) -> TardisResult<Api<Gateway>> {
        Ok(Api::namespaced(get_k8s_client().await?, namespace.as_ref().unwrap_or(&DEFAULT_NAMESPACE.to_string())))
    }
}
//old version
// impl GatewayService {
//     pub async fn list(namespace: Option<String>, query: GatewayQueryDto) -> TardisResult<Vec<SgGateway>> {
//         let mut result = vec![];
//         #[cfg(feature = "k8s")]
//         {
//             let gateway_api: Api<Gateway> = if let Some(namespace) = &namespace {
//                 Api::namespaced(get_k8s_client().await?, namespace)
//             } else {
//                 Api::all(get_k8s_client().await?)
//             };

//             let gateway_list = gateway_api.list(&ListParams::default().fields(&query.to_fields())).await.map_err(|e| TardisError::io_error(&format!("err:{e}"), ""))?;

//             result = Self::kube_to(
//                 gateway_list
//                     .items
//                     .into_iter()
//                     .filter(|g| {
//                         query.hostname.as_ref().map(|hostname| g.spec.listeners.iter().any(|l| l.hostname == Some(hostname.to_string()))).unwrap_or(true)
//                             && query.port.map(|port| g.spec.listeners.iter().any(|l| l.port == port)).unwrap_or(true)
//                     })
//                     .collect(),
//             )
//             .await?;
//         }
//         #[cfg(not(feature = "k8s"))]
//         {}

//         Ok(result)
//     }

//     pub async fn add(namespace: Option<String>, add: SgGateway) -> TardisResult<SgGateway> {
//         let result;
//         #[cfg(feature = "k8s")]
//         {
//             let namespace = namespace.unwrap_or(DEFAULT_NAMESPACE.to_string());

//
//         }
//         #[cfg(not(feature = "k8s"))]
//         {
//             result = add;
//         }
//         Ok(result)
//     }

//     pub async fn edit(namespace: Option<String>, edit: SgGateway) -> TardisResult<SgGateway> {
//         #[cfg(feature = "k8s")]
//         {
//             let _gateway_api: Api<Gateway> = Self::get_gateway_api(&namespace).await?;

//             //todo 对比z
//             // let (gateway, secrets, sgfilters) = edit.to_kube_gateway();
//             // gateway_api.replace(&edit.name, &DeleteParams::default(), gateway).await.map_err(|e| TardisError::io_error(&format!("[SG.admin] delete error:{e}"), ""))?;
//         }
//         #[cfg(not(feature = "k8s"))]
//         {}
//         Ok(edit)
//     }
//     //todo try to compress with kernel::config::config_by_k8s
//     #[cfg(feature = "k8s")]
//     pub async fn kube_to(gateway_list: Vec<Gateway>) -> TardisResult<Vec<SgGateway>> {
//         let mut result = vec![];
//         for g in gateway_list {
//             result.push(SgGateway {
//                 name: g.name_any(),
//                 parameters: SgParameters::from_kube_gateway(&g),
//                 listeners: join_all(
//                     g.spec
//                         .listeners
//                         .into_iter()
//                         .map(|listener| async move {
//                             let tls = match listener.tls {
//                                 Some(tls) => {
//                                     let certificate_ref = tls
//                                         .certificate_refs
//                                         .as_ref()
//                                         .ok_or_else(|| TardisError::format_error("[SG.Config] Gateway [spec.listener.tls.certificateRefs] is required", ""))?
//                                         .get(0)
//                                         .ok_or_else(|| TardisError::format_error("[SG.Config] Gateway [spec.listener.tls.certificateRefs] is empty", ""))?;
//                                     let secret_api: Api<Secret> =
//                                         Api::namespaced(get_k8s_client().await?, certificate_ref.namespace.as_ref().unwrap_or(&DEFAULT_NAMESPACE.to_string()));
//                                     if let Some(secret_obj) = secret_api
//                                         .get_opt(&certificate_ref.name)
//                                         .await
//                                         .map_err(|error| TardisError::wrap(&format!("[SG.Config] Kubernetes error: {error:?}"), ""))?
//                                     {
//                                         let secret_data = secret_obj
//                                             .data
//                                             .ok_or_else(|| TardisError::format_error(&format!("[SG.Config] Gateway tls secret [{}] data is required", certificate_ref.name), ""))?;
//                                         let tls_crt = secret_data.get("tls.crt").ok_or_else(|| {
//                                             TardisError::format_error(&format!("[SG.Config] Gateway tls secret [{}] data [tls.crt] is required", certificate_ref.name), "")
//                                         })?;
//                                         let tls_key = secret_data.get("tls.key").ok_or_else(|| {
//                                             TardisError::format_error(&format!("[SG.Config] Gateway tls secret [{}] data [tls.key] is required", certificate_ref.name), "")
//                                         })?;
//                                         Some(SgTlsConfig {
//                                             mode: SgTlsMode::from(tls.mode).unwrap_or_default(),
//                                             key: String::from_utf8(tls_key.0.clone()).expect("[SG.Config] Gateway tls secret [tls.key] is not valid utf8"),
//                                             cert: String::from_utf8(tls_crt.0.clone()).expect("[SG.Config] Gateway tls secret [tls.cert] is not valid utf8"),
//                                         })
//                                     } else {
//                                         TardisError::not_found(&format!("[SG.admin] Gateway have tls secret [{}], but not found!", certificate_ref.name), "");
//                                         None
//                                     }
//                                 }
//                                 None => None,
//                             };
//                             let sg_listener = SgListener {
//                                 name: listener.name,
//                                 ip: None,
//                                 port: listener.port,
//                                 protocol: match listener.protocol.to_lowercase().as_str() {
//                                     "http" => SgProtocol::Http,
//                                     "https" => SgProtocol::Https,
//                                     "ws" => SgProtocol::Ws,
//                                     _ => {
//                                         return Err(TardisError::not_implemented(
//                                             &format!("[SG.Config] Gateway [spec.listener.protocol={}] not supported yet", listener.protocol),
//                                             "",
//                                         ))
//                                     }
//                                 },
//                                 tls,
//                                 hostname: listener.hostname,
//                             };
//                             Ok(sg_listener)
//                         })
//                         .collect::<Vec<_>>(),
//                 )
//                 .await
//                 .into_iter()
//                 .map(|listener| listener.expect("[SG.Config] Unexpected none: listener"))
//                 .collect(),
//                 filters: None,
//             })
//         }
//         Ok(result)
//     }
// }
