#![allow(non_snake_case)]

use std::sync::Arc;

use operit_node_runtime::remote::{LinkAccessIdentity, LinkAccessStore, LinkDeviceInfo};
#[cfg(not(target_arch = "wasm32"))]
use operit_node_runtime::remote::{RemoteLinkServer, RemoteLinkServerConfig, RemoteWebAccessConfig};
use operit_host_api::HostManager::HostManager;
use operit_host_api::PluginSdkIpc::PluginSdkIpcEndpoint;
use operit_node_runtime::{
    CoreNodeRouter::{CoreNodeLocalRuntime, CoreNodeRouter},
    RuntimeRemoteLinkService::RuntimeRemoteLinkService,
};
use operit_proxy_local::LocalCoreProxy;
use operit_plugin_sdk_ipc::PluginSdkLinkTarget;
use operit_plugin_sdk_ipc_bridge::OperitPluginSdkIpcBridge;
use operit_runtime::core::application::OperitApplication::OperitApplication;

type LocalClientConfigurator = Box<dyn FnOnce(&mut LocalCoreProxy) -> Result<(), String> + Send>;

/// Contains the host-provided inputs needed to start one Core tree.
pub struct CoreApplicationConfig {
    pub hostManager: HostManager,
    pub deviceInfo: LinkDeviceInfo,
    startSpaceSync: bool,
    localClientConfigurator: Option<LocalClientConfigurator>,
}

impl CoreApplicationConfig {
    /// Creates a Core application config from host capabilities and device identity.
    pub fn new(hostManager: HostManager, deviceInfo: LinkDeviceInfo) -> Self {
        Self {
            hostManager,
            deviceInfo,
            startSpaceSync: true,
            localClientConfigurator: None,
        }
    }

    /// Selects whether this Core application owns the persistent Space synchronizer.
    #[allow(non_snake_case)]
    pub fn withSpaceSync(mut self, enabled: bool) -> Self {
        self.startSpaceSync = enabled;
        self
    }

    /// Adds a setup hook that runs before the local client is shared by the Core tree.
    #[allow(non_snake_case)]
    pub fn withLocalClientConfigurator(
        mut self,
        configurator: impl FnOnce(&mut LocalCoreProxy) -> Result<(), String> + Send + 'static,
    ) -> Self {
        self.localClientConfigurator = Some(Box::new(configurator));
        self
    }
}

/// Describes the remote Link server owned by one Core application.
#[cfg(not(target_arch = "wasm32"))]
pub struct CoreRemoteLinkServerConfig {
    pub bindAddress: String,
    pub token: String,
    pub webAccess: Option<RemoteWebAccessConfig>,
    pub printStartupInfo: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl CoreRemoteLinkServerConfig {
    /// Creates the remote Link server config used by a Core application.
    pub fn new(bindAddress: String, token: String) -> Self {
        Self {
            bindAddress,
            token,
            webAccess: None,
            printStartupInfo: true,
        }
    }

    /// Attaches a Web Access surface to the remote Link server.
    #[allow(non_snake_case)]
    pub fn withWebAccess(mut self, webAccess: RemoteWebAccessConfig) -> Self {
        self.webAccess = Some(webAccess);
        self
    }

    /// Sets whether the server prints startup information.
    #[allow(non_snake_case)]
    pub fn withStartupInfo(mut self, printStartupInfo: bool) -> Self {
        self.printStartupInfo = printStartupInfo;
        self
    }
}

/// Owns the running Core tree and exposes narrow handles to host surfaces.
pub struct CoreApplication {
    localClient: Arc<LocalCoreProxy>,
    nodeRuntime: CoreNodeLocalRuntime,
    nodeRouter: CoreNodeRouter,
    accessStore: LinkAccessStore,
    accessIdentity: LinkAccessIdentity,
    accessServices: RuntimeRemoteLinkService,
    pluginSdkIpcBridge: Option<OperitPluginSdkIpcBridge>,
}

impl CoreApplication {
    /// Starts one Core tree from explicit host and access configuration.
    pub async fn start(config: CoreApplicationConfig) -> Result<Self, String> {
        let startSpaceSync = config.startSpaceSync;
        let mut runtimeApplication = OperitApplication::newWithContext(config.hostManager);
        runtimeApplication.onCreate()?;
        let mut localClient = LocalCoreProxy::new(runtimeApplication);
        if let Some(configurator) = config.localClientConfigurator {
            configurator(&mut localClient)?;
        }
        Self::startWithSharedLocalClientConfigured(
            Arc::new(localClient),
            config.deviceInfo,
            startSpaceSync,
        )
    }

    /// Starts one Core tree from a configured local client owned by the caller until this point.
    #[allow(non_snake_case)]
    pub fn startWithLocalClient(
        localClient: LocalCoreProxy,
        deviceInfo: LinkDeviceInfo,
    ) -> Result<Self, String> {
        Self::startWithSharedLocalClient(Arc::new(localClient), deviceInfo)
    }

    /// Starts one Core tree from a shared local client handle.
    #[allow(non_snake_case)]
    pub fn startWithSharedLocalClient(
        localClient: Arc<LocalCoreProxy>,
        deviceInfo: LinkDeviceInfo,
    ) -> Result<Self, String> {
        Self::startWithSharedLocalClientConfigured(localClient, deviceInfo, true)
    }

    /// Starts one Core tree while explicitly selecting whether its persistence worker is owned here.
    #[allow(non_snake_case)]
    fn startWithSharedLocalClientConfigured(
        localClient: Arc<LocalCoreProxy>,
        deviceInfo: LinkDeviceInfo,
        startSpaceSync: bool,
    ) -> Result<Self, String> {
        let nodeRuntime = localClient.coreNodeLocalRuntime();
        let nodeRouter = CoreNodeRouter::new(nodeRuntime.clone());
        let accessStore = LinkAccessStore::new(nodeRuntime.runtimeStorageHost());
        let accessIdentity = accessStore.initializeIdentity(deviceInfo)?;
        let accessServices = RuntimeRemoteLinkService::newWithAccessStore(
            nodeRuntime.clone(),
            nodeRouter.clone(),
            accessStore.clone(),
        );
        let routeChangeServices = accessServices.clone();
        localClient.bindCoreRouteChangeHandler(Arc::new(
            move |chatId, targetNodeId, resumeContext| {
                let services = routeChangeServices.clone();
                Box::pin(async move {
                    services
                        .requestChangeRoute(chatId, targetNodeId, resumeContext)
                        .await
                })
            },
        ))?;
        accessServices.startConnections()?;
        if startSpaceSync {
            if let Err(error) = accessServices.startSpaceSync() {
                let _ = accessServices.stopConnections();
                return Err(error);
            }
        }
        let pluginSdkIpcBridge = localClient
            .hostManager()
            .pluginSdkIpcHost
            .clone()
            .map(|host| {
                let target: Arc<dyn PluginSdkLinkTarget> = Arc::new(nodeRouter.clone());
                OperitPluginSdkIpcBridge::new(
                    host,
                    PluginSdkIpcEndpoint::standard(),
                    target,
                    operit_proxy_local::pluginSdkSurface(),
                )
            });
        if let Some(bridge) = &pluginSdkIpcBridge {
            bridge.start().map_err(|error| error.to_string())?;
        }
        Ok(Self {
            localClient,
            nodeRuntime,
            nodeRouter,
            accessStore,
            accessIdentity,
            accessServices,
            pluginSdkIpcBridge,
        })
    }

    /// Returns the generated local Core client entry point.
    pub fn localClient(&self) -> Arc<LocalCoreProxy> {
        self.localClient.clone()
    }

    /// Returns the local runtime capability handle owned by the node tree.
    pub fn nodeRuntime(&self) -> CoreNodeLocalRuntime {
        self.nodeRuntime.clone()
    }

    /// Returns the router that owns Rust route dispatch for this Core tree.
    pub fn nodeRouter(&self) -> CoreNodeRouter {
        self.nodeRouter.clone()
    }

    /// Returns the Link Access store owned by this Core tree.
    pub fn accessStore(&self) -> LinkAccessStore {
        self.accessStore.clone()
    }

    /// Returns the initialized Link Access identity for this Core tree.
    pub fn accessIdentity(&self) -> &LinkAccessIdentity {
        &self.accessIdentity
    }

    /// Updates the Link Access device information owned by this Core tree.
    #[allow(non_snake_case)]
    pub fn updateAccessIdentity(
        &mut self,
        deviceInfo: LinkDeviceInfo,
    ) -> Result<LinkAccessIdentity, String> {
        let accessIdentity = self.accessStore.updateIdentityDeviceInfo(deviceInfo)?;
        self.accessIdentity = accessIdentity.clone();
        Ok(accessIdentity)
    }

    /// Returns the Access service facade owned by this Core tree.
    pub fn accessServices(&self) -> RuntimeRemoteLinkService {
        self.accessServices.clone()
    }

    /// Serves the application-owned authenticated remote Link endpoint.
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(non_snake_case)]
    pub async fn serveRemoteLink(&self, config: CoreRemoteLinkServerConfig) -> Result<(), String> {
        RemoteLinkServer::serve(
            self.nodeRouter.clone(),
            RemoteLinkServerConfig {
                bindAddress: config.bindAddress,
                token: config.token,
                deviceId: self.accessIdentity.deviceId.clone(),
                deviceInfo: self.accessIdentity.deviceInfo.clone(),
                webAccess: config.webAccess,
                printStartupInfo: config.printStartupInfo,
                accessStore: self.accessStore.clone(),
            },
        )
        .await
    }

    /// Stops application-owned global route state.
    pub async fn shutdown(self) {
        self.shutdownNow();
    }

    /// Stops application-owned global route state from a synchronous host boundary.
    #[allow(non_snake_case)]
    pub fn shutdownNow(self) {
        if let Some(bridge) = &self.pluginSdkIpcBridge {
            let _ = bridge.stop();
        }
        let _ = self.accessServices.stopSpaceSync();
        let _ = self.accessServices.stopConnections();
        operit_link::clearCoreRouteRuntime();
    }
}

impl Drop for CoreApplication {
    /// Releases process-local synchronization ownership when a Core tree is dropped.
    fn drop(&mut self) {
        if let Some(bridge) = &self.pluginSdkIpcBridge {
            let _ = bridge.stop();
        }
        let _ = self.accessServices.stopSpaceSync();
        let _ = self.accessServices.stopConnections();
    }
}
