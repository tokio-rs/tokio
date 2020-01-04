use std::net;
pub(crate) fn find_unused_ipaddr(used: &Vec<net::IpAddr>) -> net::IpAddr {
    let mut used = used.clone();
    used.sort();
    let mut candidate_ip: net::Ipv4Addr;
    match used.last() {
        Some(net::IpAddr::V4(v)) => {
            candidate_ip = *v;
        }
        _ => {
            candidate_ip = net::Ipv4Addr::new(10, 0, 0, 0);
        }
    };

    loop {
        if used.contains(&candidate_ip.into()) {
            let octets = candidate_ip.octets();
            let mut first = octets[3];
            let mut second = octets[2];
            let mut third = octets[1];
            if first == 255u8 {
                first = 0;
                second += 1;
            }
            if second == 255u8 {
                second = 0;
                third += 1;
            }
            if third == 255 {
                panic!("out of ip addrs");
            }
            first += 1;
            candidate_ip = net::Ipv4Addr::new(octets[0], third, second, first);
        } else {
            return candidate_ip.into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unused_ipaddr() {
        let mut ipaddrs: Vec<net::IpAddr> = Vec::new();
        ipaddrs.push(net::Ipv4Addr::new(10, 0, 0, 0).into());
        ipaddrs.push(net::Ipv4Addr::new(10, 0, 0, 1).into());
        ipaddrs.push(net::Ipv4Addr::new(10, 0, 0, 2).into());
        assert_eq!(
            net::Ipv4Addr::new(10, 0, 0, 3),
            find_unused_ipaddr(&ipaddrs)
        );
    }
}
