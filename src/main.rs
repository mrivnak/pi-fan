use rppal::pwm;
use serde::Deserialize;
use std::collections::HashMap;
use std::{fs, thread, time};

const FAIL_TEMP: i32 = -100;
const FAIL_SPEED: f32 = 50.0;

#[derive(Deserialize)]
struct Config {
    settings: Settings,
    fan_curve: RawCurve,
}

#[derive(Deserialize)]
struct Settings {
    update_rate: f32, // update rate in seconds
}

#[derive(Deserialize)]
struct RawCurve {
    raw_curve: Vec<(i32, i32)>,
}

struct Curve {
    curve: HashMap<i32, i32>,
}

impl From<Vec<(i32, i32)>> for Curve {
    fn from(items: Vec<(i32, i32)>) -> Self {
        let mut curve = HashMap::new();
        for (temp, speed) in items.into_iter() {
            curve.insert(temp, speed);
        }
        Curve { curve }
    }
}

impl Curve {
    fn get_value_at(&self, temp: i32) -> f32 {
        if self.curve.contains_key(&temp) {
            *(self.curve.get(&temp).unwrap()) as f32
        } else {
            let mut keys: Vec<i32> = self.curve.keys().cloned().collect();
            keys.sort();

            let first = keys.first().unwrap();
            let last = keys.last().unwrap();

            if &temp <= first {
                *(self.curve.get(first).unwrap()) as f32
            } else if &temp >= last {
                *(self.curve.get(first).unwrap()) as f32
            } else {
                let mut x1 = keys[0];
                let mut x2 = keys[1];

                for i in 2..keys.len() {
                    if x1 <= temp && temp < x2 {
                        break;
                    } else {
                        x1 = x2;
                        x2 = keys[i];
                    }
                }

                self.get_value_between_points(x1, x2, temp)
            }
        }
    }

    fn get_value_between_points(&self, x1: i32, x2: i32, temp: i32) -> f32 {
        let y1 = *(self.curve.get(&x1).unwrap());
        let y2 = *(self.curve.get(&x2).unwrap());
        let slope = (y2 - y1) as f32 / (x2 - x1) as f32;
        let y = slope * (temp - x1) as f32 + y1 as f32;
        y
    }
}

fn get_temp() -> i32 {
    fs::read_to_string("/sys/class/thermal/thermal_zone0/temp")
        .expect("Failed to read temp")
        .trim()
        .parse::<i32>()
        .unwrap_or(FAIL_TEMP) / 1000
}

fn get_speed(temp: i32, curve: &Curve) -> f32 {
    if temp == FAIL_TEMP {
        FAIL_SPEED
    } else {
        curve.get_value_at(temp)
    }
}

fn update_speed(pin: &pwm::Pwm, curve: &Curve) {
    let temp = get_temp();
    let speed = get_speed(temp, &curve);

    pin.set_duty_cycle((speed * 256.0) as f64).unwrap();
}

fn main() {
    let config_path = if cfg!(debug_assertions) {
        String::from("res/config.toml")
    } else {
        String::from("/etc/pi-fan.toml")
    };

    let config_file = std::fs::read_to_string(config_path).unwrap();
    let config: Config = toml::from_str(config_file.as_str()).unwrap();
    let curve: Curve = Curve::from(config.fan_curve.raw_curve);

    let pwm_pin = pwm::Pwm::with_frequency(
        pwm::Channel::Pwm0,
        25000.0,
        0.0,
        pwm::Polarity::Normal,
        true,
    )
    .unwrap();

    loop {
        update_speed(&pwm_pin, &curve);
        thread::sleep(time::Duration::from_millis(
            (config.settings.update_rate * 1000.0) as u64,
        ));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn basic_speed() {
        let curve = vec![
            (0, 0),
            (10, 100),
            (20, 200),
            (30, 300),
            (40, 400),
            (50, 500)
        ];
        let curve = super::Curve::from(curve);
        assert_eq!(curve.get_value_at(0), 0.0);
        assert_eq!(curve.get_value_at(10), 100.0);
        assert_eq!(curve.get_value_at(20), 200.0);
        assert_eq!(curve.get_value_at(30), 300.0);
        assert_eq!(curve.get_value_at(40), 400.0);
        assert_eq!(curve.get_value_at(50), 500.0);
    }

    #[test]
    fn linear_speed() {
        let curve = vec![
            (0, 0),
            (10, 100),
            (20, 200),
            (30, 300),
            (40, 400),
            (50, 500),
        ];
        let curve = super::Curve::from(curve);
        assert_eq!(curve.get_value_at(5), 50.0);
        assert_eq!(curve.get_value_at(15), 150.0);
        assert_eq!(curve.get_value_at(25), 250.0);
        assert_eq!(curve.get_value_at(35), 350.0);
        assert_eq!(curve.get_value_at(45), 450.0);
    }
    #[test]
    fn quadratic_speed() {
        let curve = vec![
            (0, 0),
            (10, 100),
            (20, 300),
            (30, 700)
        ];
        let curve = super::Curve::from(curve);
        assert_eq!(curve.get_value_at(5), 50.0);
        assert_eq!(curve.get_value_at(15), 200.0);
        assert_eq!(curve.get_value_at(25), 500.0);
    }
}
