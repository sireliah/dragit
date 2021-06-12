/// This module provides Linux version of Dragit with runtime firewalld check
/// of services and ports required for the application to work.
///
/// Integration is done through D-Bus interfaces of firewalld:
/// https://firewalld.org/documentation/man-pages/firewalld.dbus.html
///
/// In order to avoid asking user for authorization every time Dragit is ran,
/// we add ports to the permanent configuration.
use std::collections::HashMap;
use std::error::Error;

use zbus::dbus_proxy;
use zbus::{self, Connection};
use zvariant::OwnedObjectPath;

use crate::user_data::UserConfig;

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
        let mdns_enabled = proxy.query_service("", "mdns")?;
        let port_enabled = proxy.query_port("", &port.to_string(), "tcp")?;

        let (mdns_needed, port_needed) = (!mdns_enabled, !port_enabled);
        info!(
            "Firewalld check, need do enable: Mdns: {}, Port: {}",
            mdns_needed, port_needed
        );
        Ok((mdns_needed, port_needed))
    }

    pub fn handle(&self, mdns_needed: bool, port_needed: bool) -> Result<(), Box<dyn Error>> {
        if mdns_needed || port_needed {
            let config = UserConfig::new()?;
            let port = config.get_port();
            let port_str = port.to_string();

            // Calls below will prompt user for password
            let zone_path = self.get_default_zone_object_path()?;
            let proxy_config =
                FirewallD1ConfigZoneProxy::new_for_path(&self.connection, &zone_path)?;

            if port_needed {
                proxy_config.add_port(&port_str, "tcp")?;
            }

            if mdns_needed {
                match proxy_config.add_service("mdns") {
                    Ok(_) => info!("Service added"),
                    Err(e) => {
                        let error = e.to_string();
                        if error.contains("ALREADY_ENABLED") {
                            warn!("Mdns was already enabled in permanent config");
                        } else {
                            error!("Service error: {}", error);
                            return Err(e.into());
                        }
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
}

#[dbus_proxy(
    default_service = "org.fedoraproject.FirewallD1",
    interface = "org.fedoraproject.FirewallD1.config.zone"
)]
trait FirewallD1ConfigZone {
    #[dbus_proxy(name = "addPort")]
    fn add_port(&self, port: &str, protocol: &str) -> zbus::Result<()>;

    #[dbus_proxy(name = "addService")]
    fn add_service(&self, service: &str) -> zbus::Result<()>;
}
