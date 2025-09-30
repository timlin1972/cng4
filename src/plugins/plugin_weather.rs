use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc::Sender;

use crate::arguments::Mode;
use crate::consts;
use crate::messages::{self as msgs, Action, Data, Msg, WeatherKey};
use crate::plugins::plugins_main;
use crate::utils::{
    common,
    weather::{self, City, Weather, WeatherDaily},
};

pub const MODULE: &str = "weather";
const WEATHER_POLLING: u64 = 15 * 60; // 15 mins
const ADD_PARAMS: &str = "<name> <latitude> <longitude>";

#[derive(Debug)]
pub struct Plugin {
    msg_tx: Sender<Msg>,
    mode: Mode,
    gui_panel: Option<String>,
    cities: Vec<City>,
}

impl Plugin {
    pub async fn new(msg_tx: Sender<Msg>, mode: Mode) -> Result<Self> {
        let myself = Self {
            msg_tx,
            mode,
            gui_panel: None,
            cities: Vec::new(),
        };

        myself.info(consts::NEW.to_string()).await;
        myself.init().await;

        Ok(myself)
    }

    async fn init(&self) {
        self.info(consts::INIT.to_string()).await;

        // let mut shutdown_rx = self.shutdown_tx.subscribe();
        let msg_tx_clone = self.msg_tx.clone();
        tokio::spawn(async move {
            msgs::cmd(
                &msg_tx_clone,
                MODULE,
                &format!("{} {MODULE} {}", consts::P, Action::Update),
            )
            .await;
            loop {
                tokio::select! {
                    // _ = shutdown_rx.recv() => {
                    //     break;
                    // }
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(WEATHER_POLLING)) => {
                        msgs::cmd(&msg_tx_clone, MODULE, &format!("{} {MODULE} {}", consts::P, Action::Update)).await;
                    }
                }
            }
        });
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
        self.info(format!("  Mode: {}", self.mode)).await;
        self.info(format!("  Gui panel: {:?}", self.gui_panel))
            .await;
        self.info(format!("  {:<12} {:<7}", "Name", "Temp")).await;
        for city in &self.cities {
            self.info(format!(
                "  {} {:<7}",
                common::pad_str(&city.name, 12),
                common::temperature_str(city.weather.as_ref().map(|w| w.temperature))
            ))
            .await;
        }
    }

    async fn handle_cmd_help(&self) {
        self.info(Action::Help.to_string()).await;
        self.info(format!("  {}", Action::Help)).await;
        self.info(format!("  {}", Action::Show)).await;
        self.info(format!("  {} <gui_panel>", Action::Gui)).await;
        self.info(format!("  {} {ADD_PARAMS}", Action::Add)).await;
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

    // p weather update
    async fn handle_cmd_update_cities(&mut self) {
        let cities = self.cities.clone();
        let msg_tx_clone = self.msg_tx.clone();
        let mode = self.mode.clone();
        let gui_panel_clone = self.gui_panel.clone();

        tokio::spawn(async move {
            for city in cities {
                let weather = weather::get_weather(city.latitude, city.longitude).await;

                if let Ok(weather) = weather {
                    let (time, temperature, weathercode) =
                        (weather.time, weather.temperature, weather.weathercode);
                    let city_name = &city.name;

                    msgs::cmd(
                        &msg_tx_clone,
                        MODULE,
                        &format!(
                            "{} {MODULE} {} {} {city_name} {time} {temperature} {weathercode}",
                            consts::P,
                            Action::Update,
                            WeatherKey::Summary,
                        ),
                    )
                    .await;

                    // update infos
                    if mode == Mode::Gui
                        && let Some(gui_panel_clone) = &gui_panel_clone
                    {
                        msgs::cmd(
                            &msg_tx_clone,
                            MODULE,
                            &format!(
                                "{} {gui_panel_clone} {} {MODULE} {} {city_name} {time} {temperature} {weathercode}",
                                consts::P,
                                Action::Update,
                                WeatherKey::Summary,
                            ),
                        )
                        .await;
                    }

                    for (idx, daily) in weather.daily.iter().enumerate() {
                        let (
                            time,
                            temperature_2m_max,
                            temperature_2m_min,
                            precipitation_probability_max,
                            weather_code,
                        ) = (
                            &daily.time,
                            daily.temperature_2m_max,
                            daily.temperature_2m_min,
                            daily.precipitation_probability_max,
                            daily.weather_code,
                        );

                        msgs::cmd(
                            &msg_tx_clone,
                            MODULE,
                            &format!(
                                "{} {MODULE} {} {} {city_name} {idx} {time} {temperature_2m_max} {temperature_2m_min} {precipitation_probability_max} {weather_code}",
                                consts::P,
                                Action::Update,
                                WeatherKey::Daily,
                            ),
                        )
                        .await;

                        // update infos
                        if mode == Mode::Gui
                            && let Some(gui_panel_clone) = &gui_panel_clone
                        {
                            msgs::cmd(
                                &msg_tx_clone,
                                MODULE,
                                &format!(
                                    "{} {gui_panel_clone} {} {MODULE} {} {city_name} {idx} {time} {temperature_2m_max} {temperature_2m_min} {precipitation_probability_max} {weather_code}",
                                    consts::P,
                                    Action::Update,
                                    WeatherKey::Daily,
                                ),
                            )
                            .await;
                        }
                    }
                }
            }
        });
    }

    // p weather update summary ...
    async fn handle_cmd_update_summary(&mut self, cmd_parts: Vec<String>) {
        #[allow(clippy::collapsible_if)]
        if let (Some(city_name), Some(time), Some(temperature), Some(weathercode)) = (
            cmd_parts.get(4),
            cmd_parts.get(5),
            cmd_parts.get(6),
            cmd_parts.get(7),
        ) {
            if let Some(city) = self.cities.iter_mut().find(|city| city.name == *city_name) {
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
            }
        }
    }

    // p weather update daily ...
    async fn handle_cmd_update_daily(&mut self, cmd_parts: Vec<String>) {
        #[allow(clippy::collapsible_if)]
        if let (
            Some(city_name),
            Some(idx),
            Some(time),
            Some(temperature_2m_max),
            Some(temperature_2m_min),
            Some(precipitation_probability_max),
            Some(weather_code),
        ) = (
            cmd_parts.get(4),
            cmd_parts.get(5),
            cmd_parts.get(6),
            cmd_parts.get(7),
            cmd_parts.get(8),
            cmd_parts.get(9),
            cmd_parts.get(10),
        ) {
            if let Some(city) = self.cities.iter_mut().find(|city| city.name == *city_name) {
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
            }
        }
    }

    // p weather update
    // p weather update <weather_key> ...
    async fn handle_cmd_update(&mut self, cmd_parts: Vec<String>) {
        if let Some(weather_key) = cmd_parts.get(3) {
            match weather_key.parse::<WeatherKey>() {
                Ok(WeatherKey::Summary) => self.handle_cmd_update_summary(cmd_parts).await,
                Ok(WeatherKey::Daily) => self.handle_cmd_update_daily(cmd_parts).await,
                Err(_) => {
                    self.warn(common::MsgTemplate::InvalidParameters.format(
                        &format!("<weather_key> (`{weather_key}`)"),
                        Action::Update.as_ref(),
                        &cmd_parts.join(" "),
                    ))
                    .await
                }
            }
        } else {
            self.handle_cmd_update_cities().await;
        }
    }

    async fn handle_cmd_add(&mut self, cmd_parts: Vec<String>) {
        if let (Some(city_name), Some(latitude), Some(longitude)) =
            (cmd_parts.get(3), cmd_parts.get(4), cmd_parts.get(5))
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

                        // update infos
                        if self.mode == Mode::Gui
                            && let Some(gui_panel) = &self.gui_panel
                        {
                            self.cmd(format!(
                                "{} {gui_panel} {} {MODULE} {city_name} {latitude} {longitude}",
                                consts::P,
                                Action::Add,
                            ))
                            .await;
                        }
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
                self.warn(err).await;
                return;
            }
        };

        match action {
            Action::Help => self.handle_cmd_help().await,
            Action::Show => self.handle_cmd_show().await,
            Action::Gui => self.handle_cmd_gui(cmd_parts).await,
            Action::Update => self.handle_cmd_update(cmd_parts).await,
            Action::Add => self.handle_cmd_add(cmd_parts).await,
            _ => {
                self.warn(common::MsgTemplate::UnsupportedAction.format(action.as_ref(), "", ""))
                    .await
            }
        }
    }
}
