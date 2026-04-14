//! Kubernetes service discovery (compiled in only when the `kubernetes`
//! cargo feature is enabled).
//!
//! Mirrors the .NET `KubernetesServiceDiscovery` class: we list services
//! matching a set of label selectors and materialise one [`Endpoint`]
//! per hit by formatting the selector's URL template with the service's
//! namespace.
//!
//! The Phase 3 implementation covers the happy path (list services, build
//! URLs, done). The richer features of the .NET version — in-cluster role-
//! based auth, custom kubeconfig file, retry — can be added incrementally
//! once an operator exercises the default kubeconfig flow.

use anyhow::Context;
use kube::api::ListParams;
use kube::{Api, Client, Config};
use k8s_openapi::api::core::v1::Service;

use crate::ingestion::generic::{Endpoint, EndpointKind};

/// Description of one label-selector pattern to look for.
#[derive(Debug, Clone)]
pub struct Selector {
    pub label_selector: String,
    pub url_template: String,
    pub kind: EndpointKind,
}

/// Discover endpoints in the cluster. Errors short-circuit this call; the
/// caller is expected to log them and continue with whatever static
/// endpoints are configured.
pub async fn discover(selectors: &[Selector]) -> anyhow::Result<Vec<Endpoint>> {
    let config = Config::infer().await.context("inferring kubeconfig")?;
    let client = Client::try_from(config).context("building kube client")?;
    let services: Api<Service> = Api::all(client);

    let mut endpoints = Vec::new();
    for selector in selectors {
        let params = ListParams::default().labels(&selector.label_selector);
        let list = services.list(&params).await.with_context(|| {
            format!("listing services for selector `{}`", selector.label_selector)
        })?;
        for svc in list {
            let Some(ns) = svc.metadata.namespace.as_deref() else {
                continue;
            };
            let url = selector.url_template.replace("{0}", ns);
            endpoints.push(Endpoint {
                kind: selector.kind,
                url,
                api_key: None,
                authorization: None,
                suffix: None,
            });
        }
    }

    Ok(endpoints)
}
