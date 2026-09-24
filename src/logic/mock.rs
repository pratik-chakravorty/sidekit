//! Believable fake records for fixtures and demos. A seed always yields the
//! same data, so a set can be regenerated exactly.

use serde_json::{Map, Value};

use super::Prng;

pub const FIELDS: &[(&str, &str)] = &[
    ("id", "ID"),
    ("name", "Name"),
    ("email", "Email"),
    ("username", "Username"),
    ("phone", "Phone"),
    ("company", "Company"),
    ("job", "Job title"),
    ("address", "Street"),
    ("city", "City"),
    ("country", "Country"),
    ("birthday", "Date"),
    ("age", "Age"),
    ("price", "Price"),
    ("active", "Boolean"),
    ("ip", "IPv4"),
    ("website", "URL"),
];

const FIRST: &[&str] = &[
    "Ada", "Alan", "Grace", "Linus", "Margaret", "Dennis", "Barbara", "Ken", "Katherine", "Tim", "Radia", "Guido", "Frances", "Bjarne",
    "Hedy", "John", "Anita", "Edsger", "Sophie", "Yukihiro", "Priya", "Mateo", "Amara", "Chen", "Olga", "Kwame", "Lucía", "Omar",
];
const LAST: &[&str] = &[
    "Lovelace", "Turing", "Hopper", "Torvalds", "Hamilton", "Ritchie", "Liskov", "Thompson", "Johnson", "Berners-Lee", "Perlman",
    "van Rossum", "Allen", "Stroustrup", "Lamarr", "McCarthy", "Borg", "Dijkstra", "Wilson", "Matsumoto", "Patel", "García", "Okafor",
    "Wang", "Ivanova", "Mensah", "Fernández", "Haddad",
];
const COMPANIES: &[&str] = &[
    "Northwind Labs", "Acme Robotics", "Blue Harbor Systems", "Quartz Analytics", "Pinecone Software", "Lumen Health", "Orbit Freight",
    "Cobalt Studio", "Redwood Energy", "Tidal Payments", "Summit Learning", "Maple & Co",
];
const JOBS: &[&str] = &[
    "Software Engineer", "Product Manager", "Data Scientist", "Designer", "Site Reliability Engineer", "Engineering Manager",
    "Security Analyst", "Technical Writer", "Support Specialist", "QA Engineer", "Solutions Architect", "Developer Advocate",
];
const STREETS: &[&str] = &["Maple Street", "Oak Avenue", "Harbor Road", "Station Lane", "Mill Road", "Park Place", "Church Street", "Lake View Drive"];
const CITIES: &[(&str, &str)] = &[
    ("London", "United Kingdom"), ("Berlin", "Germany"), ("Toronto", "Canada"), ("Austin", "United States"), ("Bengaluru", "India"),
    ("Lagos", "Nigeria"), ("São Paulo", "Brazil"), ("Tokyo", "Japan"), ("Sydney", "Australia"), ("Madrid", "Spain"),
    ("Nairobi", "Kenya"), ("Seoul", "South Korea"), ("Amsterdam", "Netherlands"), ("Mexico City", "Mexico"),
];
const DOMAINS: &[&str] = &["example.com", "example.org", "example.net"];

fn pick<'a, T>(r: &mut Prng, xs: &'a [T]) -> &'a T {
    &xs[((r.next() * xs.len() as f64) as usize).min(xs.len() - 1)]
}

fn int(r: &mut Prng, lo: i64, hi: i64) -> i64 {
    lo + ((r.next() * (hi - lo + 1) as f64) as i64).min(hi - lo)
}

fn ascii(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'ã' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            c => c,
        })
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

/// `count` records with the chosen `fields` (keys from [`FIELDS`]).
pub fn records(fields: &[&str], count: usize, seed: u32) -> Vec<Value> {
    let mut r = Prng(seed);
    (0..count)
        .map(|_| {
            let first = *pick(&mut r, FIRST);
            let last = *pick(&mut r, LAST);
            let (city, country) = *pick(&mut r, CITIES);
            let user = format!("{}{}", ascii(first), &ascii(last)[..1]);
            let mut m = Map::new();
            for f in fields {
                let v: Value = match *f {
                    "id" => {
                        let mut b = [0u8; 16];
                        for x in &mut b {
                            *x = int(&mut r, 0, 255) as u8;
                        }
                        b[6] = (b[6] & 0x0f) | 0x40;
                        b[8] = (b[8] & 0x3f) | 0x80;
                        let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
                        format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32]).into()
                    }
                    "name" => format!("{first} {last}").into(),
                    "email" => format!("{}.{}@{}", ascii(first), ascii(last), pick(&mut r, DOMAINS)).into(),
                    "username" => format!("{user}{}", int(&mut r, 1, 99)).into(),
                    // 555-01xx numbers are reserved for fiction.
                    "phone" => format!("+1 555-01{:02}", int(&mut r, 0, 99)).into(),
                    "company" => (*pick(&mut r, COMPANIES)).into(),
                    "job" => (*pick(&mut r, JOBS)).into(),
                    "address" => format!("{} {}", int(&mut r, 1, 240), pick(&mut r, STREETS)).into(),
                    "city" => city.into(),
                    "country" => country.into(),
                    "birthday" => format!("{}-{:02}-{:02}", int(&mut r, 1960, 2005), int(&mut r, 1, 12), int(&mut r, 1, 28)).into(),
                    "age" => int(&mut r, 18, 80).into(),
                    "price" => serde_json::Number::from_f64(int(&mut r, 100, 99_999) as f64 / 100.0).map(Value::Number).unwrap_or_default(),
                    "active" => (r.next() < 0.7).into(),
                    // 203.0.113.0/24 is set aside for documentation.
                    "ip" => format!("203.0.113.{}", int(&mut r, 1, 254)).into(),
                    "website" => format!("https://{}.{}", ascii(last), pick(&mut r, DOMAINS)).into(),
                    _ => Value::Null,
                };
                m.insert(f.to_string(), v);
            }
            Value::Object(m)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_complete() {
        let a = records(&["id", "name", "email", "age", "ip"], 5, 42);
        assert_eq!(a, records(&["id", "name", "email", "age", "ip"], 5, 42));
        assert_ne!(a, records(&["id", "name", "email", "age", "ip"], 5, 43));
        assert_eq!(a.len(), 5);
        let r = a[0].as_object().unwrap();
        assert_eq!(r.len(), 5);
        assert!(r["email"].as_str().unwrap().contains('@'));
        assert_eq!(r["id"].as_str().unwrap().len(), 36);
        assert!(r["ip"].as_str().unwrap().starts_with("203.0.113."));
        let age = r["age"].as_i64().unwrap();
        assert!((18..=80).contains(&age));
    }
}
