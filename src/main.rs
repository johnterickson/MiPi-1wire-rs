use tokio::io::AsyncReadExt;
use std::convert::Infallible;
use std::{error::Error, net::SocketAddr, fs::File, io::{BufRead, BufReader}};
use hyper::{Body, Request, Response, Server};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Method, StatusCode};

async fn get_temps() -> Result<String,Box<dyn Error>> {
    let mut body = format!("<a updated='{}'>\n", "2020-04-24 21-28");

    let mut file = File::open("/sys/devices/w1_bus_master1/w1_master_slaves")?;
    for id in BufReader::new(file).lines() {
        let id = id?;

        let path = format!("/sys/bus/w1/devices/{}/w1_slave", id);
        let mut lines = BufReader::new(File::open(path)?).lines();
        lines.next().unwrap()?;
        let data: &str = &lines.next().unwrap()?;
        let mut tokens = data.split("=");
        tokens.next().unwrap(); // ignore
        let temp = i32::from_str_radix(tokens.next().unwrap(), 10)?;
        let temp = temp as f32;
        let temp = temp / 1000.0;
        body += "<owd>\n";
        body += "<Name>DS18B20</Name>\n";
        body += &format!("<ROMId>{}</ROMId>\n",id);
        body += &format!("<Temperature>{}</Temperature>\n",temp);
        body += "</owd>\n";
    }

    body += "</a>\n";

    println!("{}", &body);

    Ok(body)
}

async fn hello_world(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let mut response = Response::new(Body::empty());
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/details.xml") => {
            let body = get_temps().await;
            let body = match body {
                Ok(b) => b,
                Err(e) => {
                    *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                    format!("Error: {}", e)
                }
            };
            *response.body_mut() = Body::from(body);
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
        Ok::<_, hyper::Error>(service_fn(hello_world))
    });

    let server = Server::bind(&addr).serve(make_svc);

    // Run this server for... forever!
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}