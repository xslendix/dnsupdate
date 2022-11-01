use reqwest::{blocking::Client, StatusCode};
use serde_derive::Deserialize;
use std::{
    error::Error,
    fs,
    io::{self, Write},
    path::Path,
};
use tldextract::{TldExtractor, TldOption};
use toml;

trait Service {
    fn update(self, ip: &str) -> Result<(), Box<dyn Error>>;
}

#[derive(Deserialize)]
struct CloudflareService {
    api_key: String,
    account_email: String,
    domains: Vec<String>,
}

impl Service for CloudflareService {
    fn update(self, ip: &str) -> Result<(), Box<dyn Error>> {
        for subdomain in self.domains {
            let tld = TldExtractor::new(TldOption::default())
                .extract(&subdomain)
                .unwrap();

            print!("[Cloudflare] Update {subdomain}: ");
            io::stdout().flush()?;

            let mut headers = reqwest::header::HeaderMap::default();
            headers.insert("X-Auth-Email", self.account_email.clone().parse().unwrap());
            headers.insert("X-Auth-Key", self.api_key.clone().parse().unwrap());
            headers.insert("Content-Type", "application/json".parse().unwrap());

            let client = reqwest::blocking::ClientBuilder::default()
                .default_headers(headers)
                .build()?;

            // Get zoneid for current zone
            let resp = json::parse(&client.get(format!(
                    "https://api.cloudflare.com/client/v4/zones?name={}.{}&status=active&per_page=1&page=1",
                    tld.domain.as_ref().unwrap(),
                    tld.suffix.as_ref().unwrap()
                )).send()?.text()?)?;

            if resp["result"].len() < 1 {
                continue;
            }
            let zone_id = resp["result"][0]["id"].as_str().unwrap();

            // Get DNS record ID
            let sub = match tld.subdomain.as_ref() {
                Some(a) => String::from(".") + a,
                None => "".to_string(),
            };

            let resp = json::parse(
                &client
                    .get(format!(
                        "https://api.cloudflare.com/client/v4/zones/{}/dns_records?name={}{}.{}",
                        zone_id,
                        sub,
                        tld.domain.as_ref().unwrap(),
                        tld.suffix.as_ref().unwrap()
                    ))
                    .send()?
                    .text()?,
            )?;

            if resp["result"].len() < 1 {
                continue;
            }

            if !(resp["result"][0]["type"] == "A") && !(resp["result"][0]["type"] == "AAAA") {
                continue;
            }

            let proxied = resp["result"][0]["proxied"].as_bool().unwrap();

            let record_id = resp["result"][0]["id"].as_str().unwrap();

            // Update the record
            let resp = json::parse(
                &client
                    .put(format!(
                        "https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records/{}",
                        record_id
                    ))
                    .body(format!(
                        r#"{{"id":"{}","content":"{}","type":"A","name":"{}{}.{}","proxied":{}}}"#,
                        record_id,
                        ip,
                        sub,
                        tld.domain.as_ref().unwrap(),
                        tld.suffix.as_ref().unwrap(),
                        proxied
                    ))
                    .send()?
                    .text()?,
            )?;

            println!(
                "{}",
                match resp["success"].as_bool().unwrap() {
                    true => "Success",
                    false => "Fail",
                }
            );
        }

        Ok(())
    }
}

#[derive(Deserialize)]
struct YDNSService {
    user: String,
    password: String,
    domains: Vec<String>,
}

impl Service for YDNSService {
    fn update(self, ip: &str) -> Result<(), Box<dyn Error>> {
        for subdomain in self.domains {
            print!("[YDNS] Update {subdomain}: ");
            io::stdout().flush()?;

            let client = Client::new();
            let resp = client
                .get(format!(
                    "https://ydns.io/api/v1/update/?host={}&ip={}",
                    subdomain, ip
                ))
                .basic_auth(self.user.to_owned(), Some(self.password.to_owned()))
                .send()
                .unwrap()
                .status();

            println!(
                "{}",
                match resp {
                    StatusCode::OK => "Success",
                    _ => "Fail",
                }
            );
        }

        Ok(())
    }
}

#[derive(Deserialize)]
struct ServiceConfig {
    cloudflare: CloudflareService,
    ydns: YDNSService,
}

fn main() {
    let my_ip = Client::default()
        .get("https://myexternalip.com/raw")
        .send()
        .unwrap()
        .text()
        .unwrap();

    let config_paths = vec![
        Path::new(".config.toml"),
        Path::new("config.toml"),
        Path::new("/etc/dnsupdate.toml"),
    ];

    let mut config_path = Path::new("");
    for path in config_paths {
        if path.exists() {
            config_path = path;
            break;
        }
    }

    if config_path.to_str().unwrap() == "" {
        panic!("Cannot find config file!");
    }

    println!("Config file path: {}", config_path.display());

    let config: ServiceConfig =
        toml::from_str(&fs::read_to_string(config_path.to_str().unwrap()).unwrap()).unwrap();

    config.cloudflare.update(&my_ip).unwrap();
    config.ydns.update(&my_ip).unwrap();
}
