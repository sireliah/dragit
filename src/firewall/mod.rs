use std::error::Error;
/// This module provides Linux version of Dragit with runtime firewalld check
/// of services and ports required for the application to work.
///
/// Integration is done through D-Bus interfaces of firewalld:
/// https://firewalld.org/documentation/man-pages/firewalld.dbus.html
///
/// In order to avoid asking user for authorization every time Dragit is ran,
/// we add ports to the permanent configuration.
use std::{collections::HashMap, vec};

use serde::{Deserialize, Serialize};
use zbus::dbus_proxy;
use zbus::{self, Connection};
use zvariant::{derive::Type, OwnedObjectPath, Type};

use crate::user_data::UserConfig;

#[derive(Debug, Serialize, Deserialize, Type, PartialEq)]
pub struct ServiceConfig {
    version: String,
    short: String,
    description: String,
    ports: Vec<(String, String)>,
    module_names: Vec<String>,
    destinations: HashMap<String, String>,
    protocols: Vec<String>,
    source_ports: Vec<(String, String)>,
}

impl ServiceConfig {
    fn new(port: String) -> ServiceConfig {
        ServiceConfig {
            version: "".to_string(),
            short: "Dragit".to_string(),
            description: "Dragit is a local network file sharing application".to_string(),
            ports: vec![(port, "tcp".to_string())],
            module_names: vec![],
            destinations: HashMap::new(),
            protocols: vec![],
            source_ports: vec![],
        }
    }
}

pub struct Firewall {
    connection: Connection,
}

impl Firewall {
    pub fn new() -> Result<Firewall, Box<dyn Error>> {
        let connection = Connection::new_system()?;
        Ok(Firewall { connection })
    }

    pub fn check_rules_needed(&self, port: u16) -> Result<(bool, bool), Box<dyn Error>> {
        let proxy = FirewallD1ZoneProxy::new(&self.connection)?;
        let dragit_port_enabled = proxy.query_port("", &port.to_string(), "tcp")?;
        let mdns_port_enabled = proxy.query_port("", "5353", "udp")?;

        info!("Running firewalld check");

        // No need to check services if right ports are already opened
        if dragit_port_enabled && mdns_port_enabled {
            Ok((false, false))
        } else {
            let mdns_service_enabled = proxy.query_service("", "mdns")?;
            let dragit_service_enabled = match proxy.query_service("", "dragit") {
                Ok(v) => v,
                Err(e) => {
                    catch_dbus_error(e, "INVALID_SERVICE")?;
                    // This means that service is not present in permanent conf
                    false
                }
            };
            let (mdns_needed, dragit_needed) = (!mdns_service_enabled, !dragit_service_enabled);
            info!(
                "Need do enable services: mdns: {}, dragit: {}",
                mdns_needed, dragit_needed
            );
            Ok((mdns_needed, dragit_needed))
        }
    }

    pub fn handle(&self, (mdns_needed, dragit_needed): (bool, bool)) -> Result<(), Box<dyn Error>> {
        if mdns_needed || dragit_needed {
            let config = UserConfig::new()?;
            let port = config.get_port();
            let port_str = port.to_string();

            // Calls below will prompt user for password
            let zone_path = self.get_default_zone_object_path()?;
            let proxy_config_zone =
                FirewallD1ConfigZoneProxy::new_for_path(&self.connection, &zone_path)?;

            if dragit_needed {
                let proxy_config = FirewallD1ConfigProxy::new(&self.connection)?;

                let service = ServiceConfig::new(port_str);
                info!("ServiceConfig signature: {}", ServiceConfig::signature());
                match proxy_config.add_service("dragit", service) {
                    Ok(v) => info!("Service result: {:?}", v),
                    Err(e) => {
                        catch_dbus_error(e, "NAME_CONFLICT")?;
                        info!("Dragit service was already present, no need to create it");
                    }
                };

                match proxy_config_zone.add_service_zone("dragit") {
                    Ok(_) => info!("Service added"),
                    Err(e) => {
                        catch_dbus_error(e, "ALREADY_ENABLED")?;
                        info!("Dragit service was already enabled in permanent config");
                    }
                };
            }

            if mdns_needed {
                match proxy_config_zone.add_service_zone("mdns") {
                    Ok(_) => info!("Service added"),
                    Err(e) => {
                        catch_dbus_error(e, "ALREADY_ENABLED")?;
                        info!("Mdns service was already enabled in permanent config");
                    }
                };
            }

            self.reload_firewall()?;
        }
        Ok(())
    }

    fn reload_firewall(&self) -> Result<(), Box<dyn Error>> {
        let proxy = FirewallD1Proxy::new(&self.connection)?;
        info!("Reloaded firewall");
        Ok(proxy.reload()?)
    }

    fn get_default_zone_object_path(&self) -> Result<String, Box<dyn Error>> {
        let proxy = FirewallD1Proxy::new(&self.connection)?;
        let proxy_config = FirewallD1ConfigProxy::new(&self.connection)?;

        let default_zone = proxy.get_default_zone()?;
        let zone_names = proxy_config.get_zone_names()?;
        let zone_paths = proxy_config.list_zones()?;

        let mut hash = HashMap::new();
        for (path, zone) in zone_paths.iter().zip(zone_names.iter()) {
            hash.insert(zone, path);
        }
        let path = match hash.get(&default_zone) {
            Some(v) => v.to_string(),
            None => Err("Could not determine the default firewall zone")?,
        };
        info!("Default zone: {}, path: {}", default_zone, path);
        Ok(path)
    }
}

#[dbus_proxy(
    default_service = "org.fedoraproject.FirewallD1",
    default_path = "/org/fedoraproject/FirewallD1",
    interface = "org.fedoraproject.FirewallD1"
)]
trait FirewallD1 {
    #[dbus_proxy(name = "reload")]
    fn reload(&self) -> zbus::Result<()>;

    #[dbus_proxy(name = "getDefaultZone")]
    fn get_default_zone(&self) -> zbus::Result<String>;
}

#[dbus_proxy(
    default_service = "org.fedoraproject.FirewallD1",
    default_path = "/org/fedoraproject/FirewallD1",
    interface = "org.fedoraproject.FirewallD1.zone"
)]
trait FirewallD1Zone {
    #[dbus_proxy(name = "queryService")]
    fn query_service(&self, zone: &str, service: &str) -> zbus::Result<bool>;

    #[dbus_proxy(name = "queryPort")]
    fn query_port(&self, zone: &str, port: &str, protocol: &str) -> zbus::Result<bool>;

    #[dbus_proxy(name = "getZones")]
    fn get_zones(&self) -> zbus::Result<Vec<String>>;
}

#[dbus_proxy(
    default_service = "org.fedoraproject.FirewallD1",
    default_path = "/org/fedoraproject/FirewallD1/config",
    interface = "org.fedoraproject.FirewallD1.config"
)]
trait FirewallD1Config {
    #[dbus_proxy(name = "listZones")]
    fn list_zones(&self) -> zbus::Result<Vec<OwnedObjectPath>>;

    #[dbus_proxy(name = "getZoneNames")]
    fn get_zone_names(&self) -> zbus::Result<Vec<String>>;

    /// Adds new service to permanent configuration
    #[dbus_proxy(name = "addService")]
    fn add_service(&self, service: &str, config: ServiceConfig) -> zbus::Result<OwnedObjectPath>;
}

#[dbus_proxy(
    default_service = "org.fedoraproject.FirewallD1",
    interface = "org.fedoraproject.FirewallD1.config.zone"
)]
trait FirewallD1ConfigZone {
    #[dbus_proxy(name = "addPort")]
    fn add_port(&self, port: &str, protocol: &str) -> zbus::Result<()>;

    /// Enables service in zone permanent configuration
    /// If service is already there, this call will enable the service in runtime
    #[dbus_proxy(name = "addService")]
    fn add_service_zone(&self, service: &str) -> zbus::Result<()>;

    #[dbus_proxy(name = "getServices")]
    fn get_services(&self) -> zbus::Result<Vec<String>>;
}

fn catch_dbus_error(e: zbus::Error, text_to_match: &str) -> Result<(), Box<dyn Error>> {
    if e.to_string().contains(text_to_match) {
        Ok(())
    } else {
        error!("Service error: {}", e);
        Err(e.into())
    }
}
