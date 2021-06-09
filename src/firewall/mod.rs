use std::collections::HashMap;
use std::error::Error;

use zbus::dbus_proxy;
use zbus::{self, Connection};
use zvariant::OwnedObjectPath;

use crate::user_data::UserConfig;

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

fn get_default_zone_object_path(connection: &Connection) -> Result<String, Box<dyn Error>> {
    let proxy = FirewallD1Proxy::new(connection)?;
    let proxy_config = FirewallD1ConfigProxy::new(connection)?;

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

fn check_rules_needed(connection: &Connection, port: u16) -> Result<(bool, bool), Box<dyn Error>> {
    let proxy = FirewallD1ZoneProxy::new(connection)?;
    let mdns_enabled = proxy.query_service("", "mdns")?;
    let port_enabled = proxy.query_port("", &port.to_string(), "tcp")?;

    Ok((!mdns_enabled, !port_enabled))
}

fn reload_firewall(connection: &Connection) -> Result<(), Box<dyn Error>> {
    let proxy = FirewallD1Proxy::new(&connection)?;
    info!("Reloaded firewall");
    Ok(proxy.reload()?)
}

pub fn handle_firewall() -> Result<(), Box<dyn Error>> {
    let connection = Connection::new_system()?;
    let config = UserConfig::new()?;
    let port = config.get_port();
    let (mdns_needed, port_needed) = check_rules_needed(&connection, port)?;
    info!(
        "Need to change config for: Mdns: {}, Port: {}",
        mdns_needed, port_needed
    );

    if mdns_needed || port_needed {
        let port_str = port.to_string();

        // Calls below will prompt user for password
        let zone_path = get_default_zone_object_path(&connection)?;
        let proxy_config = FirewallD1ConfigZoneProxy::new_for_path(&connection, &zone_path)?;

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

        reload_firewall(&connection)?;
    }
    Ok(())
}
