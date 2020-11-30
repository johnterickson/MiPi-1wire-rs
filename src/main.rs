use std::{error::Error, net::SocketAddr, fs::File, io::{BufRead, BufReader}};
use hyper::{Body, Request, Response, Server, header::{self, HeaderValue}};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Method, StatusCode};
use time::OffsetDateTime;

type AnyError = Box<dyn Error>;
trait Sensor {
    type Id: std::fmt::Display;
    fn get_ids() -> Result<Vec<Self::Id>, AnyError>;
    fn get_celcius(id: &Self::Id) -> Result<f32, AnyError>;
}

trait Clock {
    fn now_local() -> OffsetDateTime;
}

struct RealClock {}
impl Clock for RealClock {
    fn now_local() -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

struct RealSensor {}
impl Sensor for RealSensor {
    type Id = String;
    fn get_ids() -> Result<Vec<Self::Id>, AnyError> {
        let file = File::open("/sys/devices/w1_bus_master1/w1_master_slaves")?;
        let mut ids = Vec::new();
        for id in BufReader::new(file).lines() {
            ids.push(id?);
        }
        Ok(ids)
    }
    fn get_celcius(id: &Self ::Id) -> Result<f32, AnyError> { 
        let path = format!("/sys/bus/w1/devices/{}/w1_slave", id);
        let mut lines = BufReader::new(File::open(path)?).lines();
        lines.next().ok_or("missing crc line")??;
        let data: &str = &lines.next().ok_or("missing data line")??;
        let mut tokens = data.split("=");
        tokens.next().ok_or("missing before = token")?;
        let temp = i32::from_str_radix(tokens.next().ok_or("missing after = token")?, 10)?;
        let temp = temp as f32;
        let temp = temp / 1000.0;
        Ok(temp)
    }
}

struct FakeSensor {}
impl Sensor for FakeSensor {
    type Id = String;
    fn get_ids() -> Result<Vec<Self::Id>, AnyError> { 
        Ok(vec!["id1".to_owned(), "id2".to_owned(), "id3".to_owned()])
    }
    fn get_celcius(id: &Self::Id) -> Result<f32, AnyError> { 
        match id.as_str() {
            "id1" => Ok(0.0f32),
            "id2" => Ok(100.0f32),
            "id3" => Ok(-40.0f32),
            _ => unreachable!(),
        }
    }
}

fn get_temps<C: Clock, S: Sensor>() -> Result<String, AnyError> {
    let now = C::now_local().format("%Y-%m-%d %H-%M");
    let mut body = format!("<a updated='{}'>\n", &now);

    let ids = S::get_ids()?;
    for id in &ids {
        let temp = S::get_celcius(&id)?;
        body += "<owd>\n";
        body += "<Name>DS18B20</Name>\n";
        body += &format!("<ROMId>{}</ROMId>\n",id);
        body += &format!("<Temperature>{:.1}</Temperature>\n",temp);
        body += &format!("<TemperatureF>{:.1}</TemperatureF>\n",temp*9.0/5.0 + 32.0);
        body += "</owd>\n";
    }

    body += "</a>\n";

    println!("{}", &body);

    Ok(body)
}

async fn read_temp(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let mut response = Response::new(Body::empty());
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/details.xml") => {
            let test_mode = std::env::var("TEST_MODE").as_ref().map(|s| s.as_str()) == Ok("1");
            let body = if test_mode {
                get_temps::<RealClock,FakeSensor>()
            } else {
                get_temps::<RealClock,RealSensor>()
            };
            let body = match body {
                Ok(b) => b,
                Err(e) => {
                    *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                    format!("Error: {}", e)
                }
            };
            *response.body_mut() = Body::from(body);
            response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/xml; charset=utf-8"));
        },
        _ => {
            *response.status_mut() = StatusCode::NOT_FOUND;
        },
    };

    Ok(response)
}

#[tokio::main]
async fn main() {
    let addr = SocketAddr::from(([0, 0, 0, 0], 80));

    // A `Service` is needed for every connection, so this
    // creates one from our `hello_world` function.
    let make_svc = make_service_fn(|_conn| async {
        // service_fn converts our function into a `Service`
        Ok::<_, hyper::Error>(service_fn(read_temp))
    });

    let server = Server::bind(&addr).serve(make_svc);

    // Run this server for... forever!
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::prelude::*;

    struct FakeClock {}
    impl Clock for FakeClock {
        fn now_local() -> OffsetDateTime {
            date!(2020-01-01).midnight().assume_utc()
        }
    }

    #[test]
    fn format_temps() {
        assert_eq!(
            "<a updated='2020-01-01 00-00'>\n<owd>\n<Name>DS18B20</Name>\n<ROMId>id1</ROMId>\n<Temperature>0.0</Temperature>\n<TemperatureF>32.0</TemperatureF>\n</owd>\n<owd>\n<Name>DS18B20</Name>\n<ROMId>id2</ROMId>\n<Temperature>100.0</Temperature>\n<TemperatureF>212.0</TemperatureF>\n</owd>\n<owd>\n<Name>DS18B20</Name>\n<ROMId>id3</ROMId>\n<Temperature>-40.0</Temperature>\n<TemperatureF>-40.0</TemperatureF>\n</owd>\n</a>\n",
            &get_temps::<FakeClock,FakeSensor>().unwrap());
    }
}