use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};
use std::process::Command;

use serde_json::Value;

use crate::domain::ToolOutput;
use crate::safety::truncate::truncate_observation;

pub fn fetch_url(args: &Value) -> ToolOutput {
    let Some(url) = args.get("url").and_then(Value::as_str) else {
        return ToolOutput::fail("url is required.");
    };
    let mut current = url.trim().to_string();
    for _ in 0..4 {
        if let Err(error) = ensure_public(&current) {
            return ToolOutput::fail(error);
        }
        let page = match curl_once(&current) {
            Ok(page) => page,
            Err(error) => return ToolOutput::fail(error),
        };
        if matches!(page.code, 301 | 302 | 303 | 307 | 308) {
            if page.redirect.is_empty() {
                return ToolOutput::fail("The redirect has no address.");
            }
            current = page.redirect;
            continue;
        }
        if !(200..300).contains(&page.code) {
            return ToolOutput::fail(format!("Could not fetch the page. HTTP {}", page.code));
        }
        let text = strip_html(&page.body);
        if text.trim().is_empty() {
            return ToolOutput::fail("The page had no readable text.");
        }
        return ToolOutput::ok(format!("url: {current}\n{}", truncate_observation(&text, 12_000)));
    }
    ToolOutput::fail("Too many redirects.")
}

struct CurlPage {
    code: u16,
    redirect: String,
    body: String,
}

fn curl_once(url: &str) -> Result<CurlPage, String> {
    let output = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "20",
            "--max-redirs",
            "0",
            "-A",
            "Orca",
            "-w",
            "\n__ORCA__%{http_code} %{redirect_url}",
            url,
        ])
        .output()
        .map_err(|error| format!("Could not fetch the page: {error}"))?;
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let Some((body, marker)) = text.rsplit_once("\n__ORCA__") else {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if error.is_empty() {
            "Could not fetch the page.".into()
        } else {
            format!("Could not fetch the page. {error}")
        });
    };
    let mut parts = marker.splitn(2, ' ');
    let code = parts.next().unwrap_or("0").trim().parse::<u16>().unwrap_or(0);
    let redirect = parts.next().unwrap_or("").trim().to_string();
    if code == 0 {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if error.is_empty() {
            "Could not fetch the page.".into()
        } else {
            format!("Could not fetch the page. {error}")
        });
    }
    Ok(CurlPage {
        code,
        redirect,
        body: body.to_string(),
    })
}

fn ensure_public(url: &str) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Only http or https addresses can be read.".into());
    }
    let host = url_host(url);
    if host.is_empty() || host_is_private(&host) {
        return Err("Internal addresses are not read.".into());
    }
    Ok(())
}

fn url_host(url: &str) -> String {
    let mut host = url
        .split("://")
        .nth(1)
        .unwrap_or("")
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .split('@')
        .next_back()
        .unwrap_or("")
        .to_ascii_lowercase();
    if host.starts_with('[') {
        if let Some(end) = host.find(']') {
            return host[1..end].to_string();
        }
    }
    if let Some((name, _)) = host.rsplit_once(':') {
        if !name.is_empty() && !name.contains(':') {
            host = name.to_string();
        }
    }
    host
}

fn host_is_private(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty()
        || host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host == "0.0.0.0"
        || host == "::1"
        || host == "::"
    {
        return true;
    }
    if host.chars().all(|ch| ch.is_ascii_digit()) {
        return true;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return ip_is_private(&ip);
    }
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() >= 2 && parts.iter().all(|part| part.parse::<u16>().is_ok()) {
        let first = parts[0].parse::<u16>().unwrap_or(999);
        if matches!(first, 0 | 10 | 127 | 169 | 172 | 192) {
            return true;
        }
    }
    match (host.as_str(), 0u16).to_socket_addrs() {
        Ok(addrs) => addrs.map(|addr| addr.ip()).any(|ip| ip_is_private(&ip)),
        Err(_) => false,
    }
}

fn ip_is_private(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ipv4_is_private(ip),
        IpAddr::V6(ip) => ipv6_is_private(ip),
    }
}

fn ipv4_is_private(ip: &Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || octets[0] == 0
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
}

fn ipv6_is_private(ip: &Ipv6Addr) -> bool {
    let first = ip.segments()[0];
    ip.is_loopback() || ip.is_unspecified() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
}

fn strip_html(input: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    let mut skip = false;
    let lower = input.to_ascii_lowercase();
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let rest = &lower[index..];
        if !inside && (rest.starts_with("<script") || rest.starts_with("<style")) {
            skip = true;
            inside = true;
            index += 1;
            continue;
        }
        if inside && rest.starts_with("</script>") {
            skip = false;
            inside = false;
            index += "</script>".len();
            continue;
        }
        if inside && rest.starts_with("</style>") {
            skip = false;
            inside = false;
            index += "</style>".len();
            continue;
        }
        let ch = input[index..].chars().next().unwrap_or(' ');
        if ch == '<' {
            inside = true;
        } else if ch == '>' {
            inside = false;
            if !skip {
                out.push(' ');
            }
        } else if !inside && !skip {
            out.push(ch);
        }
        index += ch.len_utf8();
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}
