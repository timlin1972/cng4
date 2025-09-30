use anyhow::Result;
use async_trait::async_trait;
use chrono::{Datelike, NaiveDate};
use tokio::sync::mpsc::Sender;

use crate::consts;
use crate::messages::{self as msgs, Action, Data, DeviceKey, InfoKey, Msg, WeatherKey};
use crate::plugins::{plugin_devices, plugin_panels, plugin_weather, plugins_main};
use crate::utils::{
    self, common,
    weather::{self, City, Weather, WeatherDaily},
};

pub const MODULE: &str = "infos";
const PAGES: usize = 3;
const ADD_PARAMS: &str = "<name> <latitude> <longitude>";
const NO_DATA: &str = "No data";

#[derive(Debug)]
pub struct Plugin {
    msg_tx: Sender<Msg>,
    gui_panel: Option<String>,
    page_idx: usize,
    sub_title: Vec<String>,
    // page 0
    devices: Vec<plugin_devices::DevInfo>,
    // page 1, 2
    cities: Vec<City>,
}

impl Plugin {
    pub async fn new(msg_tx: Sender<Msg>) -> Result<Self> {
        let myself = Self {
            msg_tx,
            gui_panel: None,
            page_idx: 0,
            sub_title: vec![
                plugin_devices::MODULE.to_string(),
                format!("{} current", plugin_weather::MODULE.to_string()),
                format!("{} daily", plugin_weather::MODULE.to_string()),
            ],
            devices: Vec::new(),
            cities: Vec::new(),
        };

        myself.info(consts::NEW.to_string()).await;

        Ok(myself)
    }

    async fn output_update(&self, msg: &str) {
        msgs::cmd(
            &self.msg_tx,
            MODULE,
            &format!(
                "{} {} {} {} '{msg}'",
                consts::P,
                plugin_panels::MODULE,
                Action::OutputUpdate,
                self.gui_panel.as_deref().unwrap(),
            ),
        )
        .await;
    }

    async fn update_devices(&self) -> String {
        let mut output = format!(
            "{:<12} {:<7} {:<10} {:16} {:<7} {:13} {:<16}",
            "Name", "Onboard", "Version", "Tailscale IP", "Temp", "App Uptime", "Last Update"
        );

        for device in &self.devices {
            output += &format!(
                "\n{:<12} {:<7} {:<10} {:16} {:<7} {:13} {:<16}",
                device.name,
                plugin_devices::onboard_str(device.onboard),
                plugin_devices::version_str(&device.version),
                plugin_devices::tailscale_ip_str(&device.tailscale_ip),
                common::temperature_str(device.temperature),
                plugin_devices::app_uptime_str(device.app_uptime),
                utils::time::ts_str_no_tz_no_sec(device.ts),
            );
        }

        output
    }

    async fn update_weather_current(&mut self) -> String {
        let mut output = format!(
            "{:<12} {:<11} {:7} {:20}",
            "City", "Update", "Temp", "Weather"
        );
        for city in &self.cities {
            let (update, temperature, weather) = match &city.weather {
                Some(weather) => (
                    utils::time::ts_str(utils::time::datetime_str_to_ts(&weather.time) as u64),
                    format!("{:.1}°C", weather.temperature),
                    weather::weather_code_str(weather.weathercode).to_owned(),
                ),
                None => (
                    consts::NA.to_owned(),
                    consts::NA.to_owned(),
                    consts::NA.to_owned(),
                ),
            };

            let city_name = common::pad_str(&city.name, 12);

            output += &format!("\n{city_name} {update:<11} {temperature:7} {weather:20}",);
        }

        output
    }

    async fn update_weather_daily(&mut self) -> String {
        fn format_date(input: &str) -> String {
            let date = NaiveDate::parse_from_str(input, "%Y-%m-%d").expect("無法解析日期");
            format!("{} {}", date.format("%m/%d"), date.weekday())
        }

        if self.cities.is_empty() {
            return NO_DATA.to_string();
        }
        if self.cities[0].weather.is_none() {
            return NO_DATA.to_string();
        }

        let mut output = String::new();

        let weather = self.cities[0].weather.as_ref().unwrap();
        output.push_str(&format!("{:<12} ", "City"));
        for (idx, daily) in weather.daily.iter().enumerate() {
            if idx == 0 {
                continue;
            }
            output.push_str(&format!("{:<27} ", format_date(&daily.time)));
        }

        for city in &self.cities {
            let city_name = common::pad_str(&city.name, 12);
            output.push_str(&format!("\n{city_name} "));

            if let Some(weather) = &city.weather {
                for (idx, daily) in weather.daily.iter().enumerate() {
                    if idx == 0 {
                        continue;
                    }
                    let (temperature, precipitation_probability_max, weather_emoji, weather) = (
                        format!(
                            "{:.0}/{:.0}",
                            daily.temperature_2m_max, daily.temperature_2m_min
                        ),
                        format!("{}%", daily.precipitation_probability_max),
                        weather::weather_code_emoji(daily.weather_code).to_owned(),
                        weather::weather_code_str(daily.weather_code).to_owned(),
                    );

                    let weather_emoji = common::pad_str(&weather_emoji, 2);
                    let temperature = common::pad_str(&temperature, 6);
                    output.push_str(&format!(
                        "{weather_emoji} {precipitation_probability_max:4} {temperature} "
                    ));
                    let weather = common::pad_str(&weather, 13);
                    output.push_str(&weather);
                }
            }
        }

        output
    }

    async fn update(&mut self) {
        // update sub_title
        let sub_title = format!(
            " - {}/{PAGES} - {}",
            self.page_idx + 1,
            self.sub_title[self.page_idx]
        );
        self.cmd(format!(
            "{} {} {} {} '{sub_title}'",
            consts::P,
            plugin_panels::MODULE,
            Action::SubTitle,
            self.gui_panel.as_deref().unwrap()
        ))
        .await;

        let output = match self.page_idx {
            0 => self.update_devices().await,
            1 => self.update_weather_current().await,
            2 => self.update_weather_daily().await,
            _ => NO_DATA.to_string(),
        };

        self.output_update(&output).await;
    }

    async fn cmd(&self, msg: String) {
        msgs::cmd(&self.msg_tx, MODULE, &msg).await;
    }

    async fn info(&self, msg: String) {
        msgs::info(&self.msg_tx, MODULE, &msg).await;
    }

    async fn warn(&self, msg: String) {
        msgs::warn(&self.msg_tx, MODULE, &msg).await;
    }

    async fn handle_cmd_show(&self) {
        self.info(Action::Show.to_string()).await;
        self.info(format!("  Gui panel: {:?}", self.gui_panel))
            .await;
        for (idx, sub_title) in self.sub_title.iter().enumerate() {
            self.info(format!("  Page {}: {sub_title}", idx + 1)).await;
        }
    }

    async fn handle_cmd_help(&self) {
        self.info(Action::Help.to_string()).await;
        self.info(format!("  {}", Action::Help)).await;
        self.info(format!("  {}", Action::Show)).await;
        self.info(format!("  {} <gui_panel>", Action::Gui)).await;
    }

    async fn handle_cmd_gui(&mut self, cmd_parts: Vec<String>) {
        if let Some(gui_panel) = cmd_parts.get(3) {
            self.gui_panel = Some(gui_panel.to_string());
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<gui_panel>",
                Action::Gui.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    async fn handle_cmd_update_devices_onboard(&mut self, cmd_parts: Vec<String>) {
        if let (Some(name), Some(onboard)) = (cmd_parts.get(5), cmd_parts.get(6)) {
            let ts = utils::time::ts();

            let onboard = onboard == "1";
            if let Some(device) = self.devices.iter_mut().find(|device| device.name == *name) {
                device.ts = ts;
                device.onboard = onboard;
            } else {
                let device_add = plugin_devices::DevInfo {
                    ts,
                    name: name.to_string(),
                    onboard,
                    version: None,
                    tailscale_ip: None,
                    temperature: None,
                    app_uptime: None,
                };
                self.devices.push(device_add.clone());
            }
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<name> <onboard>",
                Action::Update.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    async fn handle_cmd_update_devices_version(&mut self, cmd_parts: Vec<String>) {
        if let (Some(name), Some(version)) = (cmd_parts.get(5), cmd_parts.get(6)) {
            let ts = utils::time::ts();

            if let Some(device) = self.devices.iter_mut().find(|device| device.name == *name) {
                device.ts = ts;
                device.version = Some(version.to_string());
            }
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<name> <version>",
                Action::Update.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    async fn handle_cmd_update_devices_tailscale_ip(&mut self, cmd_parts: Vec<String>) {
        if let (Some(name), Some(tailscale_ip)) = (cmd_parts.get(5), cmd_parts.get(6)) {
            let ts = utils::time::ts();

            if let Some(device) = self.devices.iter_mut().find(|device| device.name == *name) {
                device.ts = ts;
                device.tailscale_ip = Some(tailscale_ip.to_string());
            }
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<name> <tailscale_ip>",
                Action::Update.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    async fn handle_cmd_update_devices_temperature(&mut self, cmd_parts: Vec<String>) {
        if let (Some(name), Some(temperature)) = (cmd_parts.get(5), cmd_parts.get(6)) {
            let ts = utils::time::ts();

            if let Some(device) = self.devices.iter_mut().find(|device| device.name == *name) {
                device.ts = ts;
                device.temperature = Some(temperature.parse::<f32>().unwrap());
                if device.temperature == Some(0.0) {
                    device.temperature = None;
                }
            }
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<name> <temperature>",
                Action::Update.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    async fn handle_cmd_update_devices_app_uptime(&mut self, cmd_parts: Vec<String>) {
        if let (Some(name), Some(app_uptime)) = (cmd_parts.get(5), cmd_parts.get(6)) {
            let ts = utils::time::ts();

            if let Some(device) = self.devices.iter_mut().find(|device| device.name == *name) {
                device.ts = ts;
                device.app_uptime = Some(app_uptime.parse::<u64>().unwrap());
            }
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<name> <app_uptime>",
                Action::Update.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    // p infos update devices <device_key> <...>
    async fn handle_cmd_update_devices(&mut self, cmd_parts: Vec<String>) {
        if let Some(device_key) = cmd_parts.get(4) {
            match device_key.parse::<DeviceKey>() {
                Ok(DeviceKey::Onboard) => self.handle_cmd_update_devices_onboard(cmd_parts).await,
                Ok(DeviceKey::Version) => self.handle_cmd_update_devices_version(cmd_parts).await,
                Ok(DeviceKey::TailscaleIp) => {
                    self.handle_cmd_update_devices_tailscale_ip(cmd_parts).await
                }
                Ok(DeviceKey::Temperature) => {
                    self.handle_cmd_update_devices_temperature(cmd_parts).await
                }
                Ok(DeviceKey::AppUptime) => {
                    self.handle_cmd_update_devices_app_uptime(cmd_parts).await
                }
                _ => {
                    self.warn(common::MsgTemplate::InvalidParameters.format(
                        &format!("<device_key> (`{device_key}`)"),
                        Action::Update.as_ref(),
                        &cmd_parts.join(" "),
                    ))
                    .await;
                }
            }
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<device_key>",
                Action::Update.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    // p infos update weather summary <...>
    async fn handle_cmd_update_weather_summary(&mut self, cmd_parts: Vec<String>) {
        if let (Some(city_name), Some(time), Some(temperature), Some(weathercode)) = (
            cmd_parts.get(5),
            cmd_parts.get(6),
            cmd_parts.get(7),
            cmd_parts.get(8),
        ) {
            if let Some(city) = self.cities.iter_mut().find(|c| c.name == *city_name) {
                let time = time.to_string();
                let temperature = temperature.parse::<f32>().unwrap();
                let weathercode = weathercode.parse::<u8>().unwrap();

                if let Some(weather) = city.weather.as_mut() {
                    weather.time = time;
                    weather.temperature = temperature;
                    weather.weathercode = weathercode;
                } else {
                    city.weather = Some(Weather {
                        time,
                        temperature,
                        weathercode,
                        daily: vec![],
                    });
                }
            } else {
                self.warn(format!("City `{city_name}` not found.")).await;
            }
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<city_name> <time> <temperature> <weathercode>",
                Action::Update.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    // p infos update weather daily <...>
    async fn handle_cmd_update_weather_daily(&mut self, cmd_parts: Vec<String>) {
        if let (
            Some(city_name),
            Some(idx),
            Some(time),
            Some(temperature_2m_max),
            Some(temperature_2m_min),
            Some(precipitation_probability_max),
            Some(weather_code),
        ) = (
            cmd_parts.get(5),
            cmd_parts.get(6),
            cmd_parts.get(7),
            cmd_parts.get(8),
            cmd_parts.get(9),
            cmd_parts.get(10),
            cmd_parts.get(11),
        ) {
            if let Some(city) = self.cities.iter_mut().find(|c| c.name == *city_name) {
                let idx = idx.parse::<usize>().unwrap();
                let daily = WeatherDaily {
                    time: time.to_string(),
                    temperature_2m_max: temperature_2m_max.parse::<f32>().unwrap(),
                    temperature_2m_min: temperature_2m_min.parse::<f32>().unwrap(),
                    precipitation_probability_max: precipitation_probability_max
                        .parse::<u8>()
                        .unwrap(),
                    weather_code: weather_code.parse::<u8>().unwrap(),
                };

                if let Some(weather) = city.weather.as_mut() {
                    if weather.daily.len() <= idx {
                        weather.daily.resize_with(idx + 1, || WeatherDaily {
                            time: String::new(),
                            temperature_2m_max: 0.0,
                            temperature_2m_min: 0.0,
                            precipitation_probability_max: 0,
                            weather_code: 0,
                        });
                    }

                    weather.daily[idx] = daily;
                }
            } else {
                self.warn(format!("City `{city_name}` not found.")).await;
            }
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<city_name> <idx> <time> <temperature_2m_max> <temperature_2m_min> <precipitation_probability_max> <weather_code>",
                Action::Update.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    // p infos update weather <weather_key> <...>
    async fn handle_cmd_update_weather(&mut self, cmd_parts: Vec<String>) {
        if let Some(weather_key) = cmd_parts.get(4) {
            match weather_key.parse::<WeatherKey>() {
                Ok(WeatherKey::Summary) => self.handle_cmd_update_weather_summary(cmd_parts).await,
                Ok(WeatherKey::Daily) => self.handle_cmd_update_weather_daily(cmd_parts).await,
                _ => {
                    self.warn(common::MsgTemplate::InvalidParameters.format(
                        &format!("<weather_key> (`{weather_key}`)"),
                        Action::Update.as_ref(),
                        &cmd_parts.join(" "),
                    ))
                    .await;
                }
            }
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<weather_key>",
                Action::Update.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    // p infos update <info_key> <...>
    async fn handle_cmd_update(&mut self, cmd_parts: Vec<String>) {
        if let Some(info_key) = cmd_parts.get(3) {
            match info_key.parse::<InfoKey>() {
                Ok(InfoKey::Devices) => self.handle_cmd_update_devices(cmd_parts).await,
                Ok(InfoKey::Weather) => self.handle_cmd_update_weather(cmd_parts).await,
                _ => {
                    self.warn(common::MsgTemplate::InvalidParameters.format(
                        &format!("<info_key> (`{info_key}`)"),
                        Action::Update.as_ref(),
                        &cmd_parts.join(" "),
                    ))
                    .await;
                }
            }
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<info_key>",
                Action::Update.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    // p infos key <key>
    async fn handle_cmd_key(&mut self, cmd_parts: Vec<String>) {
        if let Some(key) = cmd_parts.get(3) {
            match key.parse::<msgs::Key>() {
                Ok(msgs::Key::Left) => {
                    if self.page_idx > 0 {
                        self.page_idx -= 1;
                    } else {
                        self.page_idx = PAGES - 1;
                    }
                }
                Ok(msgs::Key::Right) => {
                    if self.page_idx + 1 < PAGES {
                        self.page_idx += 1;
                    } else {
                        self.page_idx = 0;
                    }
                }
                _ => (), // ignore other keys
            }
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<key>",
                Action::Key.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    // p infos add weather <city_name> <latitude> <longitude>
    async fn handle_cmd_add_weather(&mut self, cmd_parts: Vec<String>) {
        if let (Some(city_name), Some(latitude), Some(longitude)) =
            (cmd_parts.get(4), cmd_parts.get(5), cmd_parts.get(6))
        {
            match (latitude.parse::<f32>(), longitude.parse::<f32>()) {
                (Ok(latitude), Ok(longitude)) => {
                    if !self.cities.iter().any(|city| city.name == *city_name) {
                        self.cities.push(City {
                            name: city_name.to_string(),
                            latitude,
                            longitude,
                            weather: None,
                        });
                        self.info(format!(
                            "Added city: `{city_name}` ({latitude}, {longitude})"
                        ))
                        .await;
                    } else {
                        self.warn(format!("City `{city_name}` already exists."))
                            .await;
                    }
                }
                _ => {
                    self.warn(common::MsgTemplate::InvalidParameters.format(
                        ADD_PARAMS,
                        Action::Add.as_ref(),
                        &cmd_parts.join(" "),
                    ))
                    .await;
                }
            }
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                ADD_PARAMS,
                Action::Add.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }

    // p infos add <info_key> <...>
    async fn handle_cmd_add(&mut self, cmd_parts: Vec<String>) {
        if let Some(info_key) = cmd_parts.get(3) {
            match info_key.parse::<InfoKey>() {
                Ok(InfoKey::Weather) => self.handle_cmd_add_weather(cmd_parts).await,
                _ => {
                    self.warn(common::MsgTemplate::InvalidParameters.format(
                        &format!("<info_key> (`{info_key}`)"),
                        Action::Add.as_ref(),
                        &cmd_parts.join(" "),
                    ))
                    .await;
                }
            }
        } else {
            self.warn(common::MsgTemplate::MissingParameters.format(
                "<info_key>",
                Action::Add.as_ref(),
                &cmd_parts.join(" "),
            ))
            .await;
        }
    }
}

#[async_trait]
impl plugins_main::Plugin for Plugin {
    fn name(&self) -> &str {
        MODULE
    }

    async fn handle_cmd(&mut self, msg: &Msg) {
        let Data::Cmd(cmd) = &msg.data;

        let (cmd_parts, action) = match common::get_cmd_action(&cmd.cmd) {
            Ok(action) => action,
            Err(err) => {
                self.warn(err.clone()).await;
                return;
            }
        };

        // p infos help
        // p infos show
        // p infos gui <gui_panel>
        // p infos key <key>
        // p infos update <info_key> <...>
        // p infos update devices onboard <name> <0|1>
        // p infos update devices version <name> <version>
        // p infos update devices tailscale_ip <name> <tailscale_ip>
        // p infos update devices temperature <name> <temperature>
        // p infos update devices app_uptime <name> <app_uptime>
        // p infos update weather <weather_key> <...>
        // p infos add <info_key> <...>
        // p infos add weather <city_name> <latitude> <longitude>
        match action {
            Action::Help => self.handle_cmd_help().await,
            Action::Show => self.handle_cmd_show().await,
            Action::Gui => self.handle_cmd_gui(cmd_parts).await,
            Action::Key => self.handle_cmd_key(cmd_parts).await,
            Action::Update => self.handle_cmd_update(cmd_parts).await,
            Action::Add => self.handle_cmd_add(cmd_parts).await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref(), "", ""))
                    .await
            }
        }

        // update gui
        self.update().await;
    }
}
