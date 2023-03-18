use crate::proxy_state::Url;

///parse a bootstrap server string into a Url
pub fn parse_bootstrap_server(bootstrap_server: String) -> Url {
    let mut split = bootstrap_server.split(':');
    let host = split.next().unwrap();
    let port = split.next().unwrap().parse().unwrap();
    Url::new(host.to_string(), port)
}
