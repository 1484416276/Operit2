use core::net::Ipv4Addr;
use std::collections::BTreeMap;

use operit_host_api::HostManager::defaultServiceDiscoveryHost;
use operit_host_api::ServiceDiscovery::{DiscoveredService, DiscoverySubscription};
use std::sync::Arc;

const OPERIT_SERVICE_TYPE: &str = "_operit._tcp.local.";
type MdnsIpv4Rank = (u8, [u8; 4]);

/// Describes one transport endpoint resolved from an Operit mDNS service record.
#[derive(Clone, Debug)]
pub(crate) struct RuntimeRemoteDiscoveryEndpoint {
    pub deviceId: String,
    pub endpoint: String,
    pub hostname: String,
    pub port: u16,
    pub tokenHash: String,
    pub version: String,
}

/// Discovers Link-enabled runtimes visible through the native mDNS transport.
#[allow(non_snake_case)]
pub(crate) fn discoverRemoteDevices(
    timeoutMs: u64,
) -> Result<Vec<RuntimeRemoteDiscoveryEndpoint>, String> {
    let records = defaultServiceDiscoveryHost()
        .map_err(|error| error.to_string())?
        .discover(OPERIT_SERVICE_TYPE, timeoutMs)
        .map_err(|error| error.to_string())?;
    let mut devices = BTreeMap::<String, (MdnsIpv4Rank, RuntimeRemoteDiscoveryEndpoint)>::new();
    for info in records {
        if let Some(device) = discoveryEndpointFromServiceInfo(&info)? {
            devices.insert(info.fullName, device);
        }
    }
    Ok(devices.into_values().map(|(_, device)| device).collect())
}

/// Subscribes to Link-enabled runtime announcements from the native mDNS transport.
#[allow(non_snake_case)]
pub(crate) fn subscribeRemoteDeviceAnnouncements(
    onDevice: impl Fn(RuntimeRemoteDiscoveryEndpoint) + Send + Sync + 'static,
) -> Result<Box<dyn DiscoverySubscription>, String> {
    defaultServiceDiscoveryHost()
        .map_err(|error| error.to_string())?
        .subscribe(
            OPERIT_SERVICE_TYPE,
            Arc::new(move |info| match discoveryEndpointFromServiceInfo(&info) {
                Ok(Some((_, endpoint))) => onDevice(endpoint),
                Ok(None) => {}
                Err(error) => {
                    operit_util::AppLogger::AppLogger::w(
                        "RuntimeRemoteLinkDiscovery",
                        &format!("invalid service announcement: {error}"),
                    );
                }
            }),
        )
        .map_err(|error| error.to_string())
}

/// Converts one resolved mDNS service into a reachable Link endpoint.
#[allow(non_snake_case)]
fn discoveryEndpointFromServiceInfo(
    info: &DiscoveredService,
) -> Result<Option<(MdnsIpv4Rank, RuntimeRemoteDiscoveryEndpoint)>, String> {
    let mut addresses = info
        .addresses
        .iter()
        .filter_map(|address| match address {
            core::net::IpAddr::V4(address) => Some(*address),
            core::net::IpAddr::V6(_) => None,
        })
        .collect::<Vec<_>>();
    if addresses.is_empty() {
        return Ok(None);
    }
    addresses.sort_by_key(mdnsIpv4Rank);
    let selectedAddress = addresses[0];
    let selectedRank = mdnsIpv4Rank(&selectedAddress);
    let properties = &info.properties;
    let (Some(deviceId), Some(tokenHash), Some(version)) = (
        properties.get("deviceId"),
        properties.get("tokenHash"),
        properties.get("version"),
    ) else {
        return Ok(None);
    };
    Ok(Some((
        selectedRank,
        RuntimeRemoteDiscoveryEndpoint {
            deviceId: deviceId.to_string(),
            endpoint: format!("http://{}:{}", selectedAddress, info.port),
            hostname: info.hostname.as_str().to_string(),
            port: info.port,
            tokenHash: tokenHash.to_string(),
            version: version.to_string(),
        },
    )))
}

/// Assigns a deterministic preference to private IPv4 addresses for a discovered service.
#[allow(non_snake_case)]
fn mdnsIpv4Rank(address: &Ipv4Addr) -> MdnsIpv4Rank {
    let class = if address.is_link_local() {
        2
    } else if address.is_private() {
        0
    } else if address.is_loopback() {
        3
    } else if address.is_unspecified() {
        4
    } else {
        1
    };
    (class, address.octets())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies incomplete Core records do not produce a connectable endpoint.
    #[test]
    fn discovery_waits_for_txt_properties() {let mut info = DiscoveredService {
            fullName: "core._operit._tcp.local.".to_owned(), hostname: "core.local.".to_owned(), addresses: vec!["192.168.8.11".parse().unwrap()], port: 37194, properties: BTreeMap::new()};
        assert!(discoveryEndpointFromServiceInfo(&info).unwrap().is_none());
        info.properties = BTreeMap::from([
            ("deviceId".to_owned(), "core-test".to_owned()),
            ("tokenHash".to_owned(), "token-test".to_owned()),
            ("version".to_owned(), "1".to_owned()),
        ]);
        let (_, endpoint) = discoveryEndpointFromServiceInfo(&info).unwrap().unwrap();
        assert_eq!(endpoint.deviceId, "core-test");
        assert_eq!(endpoint.endpoint, "http://192.168.8.11:37194");
        assert_eq!(endpoint.tokenHash, "token-test");
    }
}
