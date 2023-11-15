#[cfg(feature = "k8s")]
use crate::helper::get_k8s_client;
use crate::model::query_dto::{GatewayQueryDto, GatewayQueryInst, ToInstance};
use crate::model::vo::gateway_vo::SgGatewayVo;
use crate::model::vo::Vo;
#[cfg(feature = "k8s")]
use k8s_gateway_api::Gateway;
#[cfg(feature = "k8s")]
use kernel_common::constants::k8s_constants::DEFAULT_NAMESPACE;
#[cfg(feature = "k8s")]
use kube::api::{DeleteParams, PostParams};
use std::collections::HashSet;

#[cfg(feature = "k8s")]
use kube::Api;

use super::base_service::VoBaseService;
use crate::model::vo_converter::VoConv;
use crate::service::plugin_service::PluginK8sService;
use kernel_common::converter::plugin_k8s_conv::SgSingeFilter;
use kernel_common::helper::k8s_helper::{format_k8s_obj_unique, parse_k8s_obj_unique, parse_k8s_unique_or_default, WarpKubeResult};
use tardis::basic::result::TardisResult;

pub struct GatewayVoService;

impl VoBaseService<SgGatewayVo> for GatewayVoService {}

impl GatewayVoService {
    pub async fn list(query: GatewayQueryInst) -> TardisResult<Vec<SgGatewayVo>> {
        Ok(Self::get_type_map().await?.into_values().into_iter().filter(|g|
            if let Some(q_name) = &query.names { q_name.iter().any(|q|q.is_match(&g.name)) } else { true }
                && if let Some(q_port) = &query.port { g.listeners.iter().any(|l| l.port.eq( q_port)) } else { true }
                && if let Some(q_hostname) = &query.hostname {
                g.listeners.iter().any(|l| if let Some(l_hostname)=&l.hostname{q_hostname.is_match(l_hostname)}else { false })
            } else { true })
            .collect())
    }
    pub async fn add(mut add: SgGatewayVo) -> TardisResult<SgGatewayVo> {
        #[cfg(feature = "k8s")]
        {
            let (namespace, raw_nmae) = parse_k8s_unique_or_default(&add.get_unique_name());
            add.name = format_k8s_obj_unique(Some(&namespace), &raw_nmae);
        }
        let add_model = add.clone().to_model().await?;
        #[cfg(feature = "k8s")]
        {
            let (namespace, _) = parse_k8s_unique_or_default(&add.get_unique_name());
            let (gateway, sgfilters) = add_model.to_kube_gateway();

            let gateway_api: Api<Gateway> = Api::namespaced(get_k8s_client().await?, &namespace);

            let _ = gateway_api.create(&PostParams::default(), &gateway).await.warp_result_by_method("Add Gateway")?;

            PluginK8sService::add_sgfilter_vec(&sgfilters.iter().collect::<Vec<_>>()).await?
        }
        Self::add_vo(add).await
    }

    pub async fn update_by_id(id: &str) -> TardisResult<SgGatewayVo> {
        let gateway_o = Self::get_by_id(&id).await?;
        GatewayVoService::update(gateway_o).await
    }

    pub async fn update(update: SgGatewayVo) -> TardisResult<SgGatewayVo> {
        let update_un = &update.get_unique_name();

        let update_sg_gateway = update.clone().to_model().await?;
        let old_sg_gateway = Self::get_by_id(&update.name).await?.to_model().await?;
        #[cfg(feature = "k8s")]
        {
            let (namespace, name) = parse_k8s_obj_unique(update_un);
            let gateway_api: Api<Gateway> = Api::namespaced(get_k8s_client().await?, &namespace);
            let (update_gateway, update_filter) = update_sg_gateway.to_kube_gateway();
            gateway_api.replace(&name, &PostParams::default(), &update_gateway).await.warp_result_by_method("Replace Gateway")?;

            Self::update_gateway_filter(old_sg_gateway.to_kube_gateway().1, update_filter).await?;
        }
        Ok(Self::update_vo(update).await?)
    }

    pub async fn delete(id: &str) -> TardisResult<()> {
        let (namespace, name) = parse_k8s_obj_unique(id);
        #[cfg(feature = "k8s")]
        {
            let gateway_api: Api<Gateway> = Self::get_gateway_api(&Some(namespace)).await?;

            gateway_api.delete(&name, &DeleteParams::default()).await.warp_result_by_method("Delete Gateway")?;

            let old_sg_gateway = Self::get_by_id(&id).await?.to_model().await?;
            let (_, f_v) = old_sg_gateway.to_kube_gateway();
            PluginK8sService::delete_sgfilter_vec(&f_v.iter().collect::<Vec<_>>()).await?;
        }
        Self::delete_vo(id).await?;

        Ok(())
    }

    #[cfg(feature = "k8s")]
    async fn update_gateway_filter(old: Vec<SgSingeFilter>, update: Vec<SgSingeFilter>) -> TardisResult<()> {
        if old.is_empty() && update.is_empty() {
            return Ok(());
        }

        let old_set: HashSet<_> = old.into_iter().collect();
        let update_set: HashSet<_> = update.into_iter().collect();

        let update_vec: Vec<_> = old_set.intersection(&update_set).collect();
        PluginK8sService::update_sgfilter_vec(&update_vec).await?;
        let add_vec: Vec<_> = update_set.difference(&old_set).collect();
        PluginK8sService::add_sgfilter_vec(&add_vec).await?;
        let delete_vec: Vec<_> = old_set.difference(&update_set).collect();
        PluginK8sService::delete_sgfilter_vec(&delete_vec).await?;

        Ok(())
    }

    #[cfg(feature = "k8s")]
    async fn get_gateway_api(namespace: &Option<String>) -> TardisResult<Api<Gateway>> {
        Ok(Api::namespaced(get_k8s_client().await?, namespace.as_ref().unwrap_or(&DEFAULT_NAMESPACE.to_string())))
    }
}
//old version
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
