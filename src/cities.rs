//! A built-in list of cities for sunrise and sunset (no network lookups).

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct City {
    pub name: &'static str,
    pub country: &'static str,
    pub lat: f64,
    pub lon: f64,
}

const fn c(name: &'static str, country: &'static str, lat: f64, lon: f64) -> City {
    City {
        name,
        country,
        lat,
        lon,
    }
}

/// Cities with their coordinates (degrees, north and east positive).
pub const CITIES: &[City] = &[
    // Iraq and the Kurdistan Region
    c("Erbil", "Iraq", 36.191, 44.009),
    c("Sulaymaniyah", "Iraq", 35.557, 45.435),
    c("Duhok", "Iraq", 36.867, 42.988),
    c("Kirkuk", "Iraq", 35.468, 44.392),
    c("Halabja", "Iraq", 35.178, 45.986),
    c("Zakho", "Iraq", 37.144, 42.687),
    c("Soran", "Iraq", 36.653, 44.544),
    c("Akre", "Iraq", 36.741, 43.893),
    c("Ranya", "Iraq", 36.255, 44.883),
    c("Koya", "Iraq", 36.083, 44.628),
    c("Chamchamal", "Iraq", 35.533, 44.833),
    c("Kalar", "Iraq", 34.629, 45.322),
    c("Penjwen", "Iraq", 35.623, 45.944),
    c("Amedi", "Iraq", 37.093, 43.487),
    c("Baghdad", "Iraq", 33.315, 44.366),
    c("Basra", "Iraq", 30.508, 47.783),
    c("Mosul", "Iraq", 36.340, 43.130),
    c("Najaf", "Iraq", 32.026, 44.346),
    c("Karbala", "Iraq", 32.616, 44.025),
    c("Nasiriyah", "Iraq", 31.052, 46.261),
    c("Hillah", "Iraq", 32.464, 44.420),
    c("Ramadi", "Iraq", 33.420, 43.300),
    c("Fallujah", "Iraq", 33.350, 43.783),
    c("Samarra", "Iraq", 34.198, 43.874),
    c("Tikrit", "Iraq", 34.607, 43.678),
    c("Baqubah", "Iraq", 33.750, 44.633),
    c("Kut", "Iraq", 32.506, 45.824),
    c("Amarah", "Iraq", 31.836, 47.145),
    c("Diwaniyah", "Iraq", 31.990, 44.925),
    c("Samawah", "Iraq", 31.317, 45.283),
    c("Sinjar", "Iraq", 36.321, 41.876),
    c("Khanaqin", "Iraq", 34.349, 45.392),
    // Middle East
    c("Tehran", "Iran", 35.689, 51.389),
    c("Tabriz", "Iran", 38.080, 46.292),
    c("Mashhad", "Iran", 36.297, 59.606),
    c("Isfahan", "Iran", 32.654, 51.668),
    c("Shiraz", "Iran", 29.592, 52.584),
    c("Sanandaj", "Iran", 35.314, 46.999),
    c("Kermanshah", "Iran", 34.314, 47.065),
    c("Urmia", "Iran", 37.553, 45.076),
    c("Ankara", "Turkey", 39.933, 32.860),
    c("Istanbul", "Turkey", 41.008, 28.978),
    c("Izmir", "Turkey", 38.423, 27.143),
    c("Diyarbakir", "Turkey", 37.914, 40.230),
    c("Van", "Turkey", 38.501, 43.373),
    c("Gaziantep", "Turkey", 37.066, 37.383),
    c("Antalya", "Turkey", 36.897, 30.713),
    c("Damascus", "Syria", 33.513, 36.292),
    c("Aleppo", "Syria", 36.202, 37.134),
    c("Qamishli", "Syria", 37.052, 41.226),
    c("Beirut", "Lebanon", 33.894, 35.502),
    c("Amman", "Jordan", 31.954, 35.911),
    c("Jerusalem", "Israel", 31.769, 35.216),
    c("Tel Aviv", "Israel", 32.085, 34.782),
    c("Gaza", "Palestine", 31.502, 34.467),
    c("Ramallah", "Palestine", 31.903, 35.204),
    c("Riyadh", "Saudi Arabia", 24.713, 46.675),
    c("Jeddah", "Saudi Arabia", 21.486, 39.193),
    c("Mecca", "Saudi Arabia", 21.389, 39.858),
    c("Medina", "Saudi Arabia", 24.467, 39.612),
    c("Dammam", "Saudi Arabia", 26.392, 49.977),
    c("Kuwait City", "Kuwait", 29.376, 47.977),
    c("Manama", "Bahrain", 26.229, 50.586),
    c("Doha", "Qatar", 25.286, 51.531),
    c("Abu Dhabi", "United Arab Emirates", 24.454, 54.377),
    c("Dubai", "United Arab Emirates", 25.205, 55.271),
    c("Muscat", "Oman", 23.588, 58.383),
    c("Sanaa", "Yemen", 15.369, 44.191),
    c("Aden", "Yemen", 12.786, 45.019),
    c("Cairo", "Egypt", 30.044, 31.236),
    c("Alexandria", "Egypt", 31.200, 29.919),
    c("Nicosia", "Cyprus", 35.186, 33.382),
    c("Yerevan", "Armenia", 40.179, 44.499),
    c("Tbilisi", "Georgia", 41.716, 44.783),
    c("Baku", "Azerbaijan", 40.409, 49.867),
    // Europe
    c("London", "United Kingdom", 51.507, -0.128),
    c("Manchester", "United Kingdom", 53.481, -2.243),
    c("Birmingham", "United Kingdom", 52.486, -1.890),
    c("Edinburgh", "United Kingdom", 55.953, -3.188),
    c("Glasgow", "United Kingdom", 55.864, -4.252),
    c("Cardiff", "United Kingdom", 51.481, -3.179),
    c("Belfast", "United Kingdom", 54.597, -5.930),
    c("Dublin", "Ireland", 53.350, -6.260),
    c("Paris", "France", 48.857, 2.352),
    c("Lyon", "France", 45.764, 4.836),
    c("Marseille", "France", 43.296, 5.370),
    c("Berlin", "Germany", 52.520, 13.405),
    c("Hamburg", "Germany", 53.551, 9.994),
    c("Munich", "Germany", 48.135, 11.582),
    c("Cologne", "Germany", 50.938, 6.960),
    c("Frankfurt", "Germany", 50.110, 8.682),
    c("Amsterdam", "Netherlands", 52.368, 4.904),
    c("Rotterdam", "Netherlands", 51.924, 4.478),
    c("Brussels", "Belgium", 50.850, 4.352),
    c("Luxembourg", "Luxembourg", 49.612, 6.130),
    c("Zurich", "Switzerland", 47.377, 8.542),
    c("Geneva", "Switzerland", 46.204, 6.143),
    c("Bern", "Switzerland", 46.948, 7.447),
    c("Vienna", "Austria", 48.208, 16.374),
    c("Rome", "Italy", 41.903, 12.496),
    c("Milan", "Italy", 45.464, 9.190),
    c("Naples", "Italy", 40.852, 14.268),
    c("Madrid", "Spain", 40.417, -3.704),
    c("Barcelona", "Spain", 41.385, 2.173),
    c("Seville", "Spain", 37.389, -5.984),
    c("Valencia", "Spain", 39.470, -0.376),
    c("Lisbon", "Portugal", 38.722, -9.139),
    c("Porto", "Portugal", 41.158, -8.629),
    c("Copenhagen", "Denmark", 55.676, 12.568),
    c("Oslo", "Norway", 59.914, 10.752),
    c("Bergen", "Norway", 60.391, 5.322),
    c("Tromso", "Norway", 69.649, 18.955),
    c("Stockholm", "Sweden", 59.329, 18.069),
    c("Gothenburg", "Sweden", 57.709, 11.975),
    c("Helsinki", "Finland", 60.170, 24.938),
    c("Reykjavik", "Iceland", 64.147, -21.942),
    c("Tallinn", "Estonia", 59.437, 24.754),
    c("Riga", "Latvia", 56.950, 24.106),
    c("Vilnius", "Lithuania", 54.687, 25.280),
    c("Warsaw", "Poland", 52.230, 21.012),
    c("Krakow", "Poland", 50.065, 19.945),
    c("Prague", "Czechia", 50.076, 14.438),
    c("Bratislava", "Slovakia", 48.149, 17.107),
    c("Budapest", "Hungary", 47.498, 19.040),
    c("Ljubljana", "Slovenia", 46.057, 14.506),
    c("Zagreb", "Croatia", 45.815, 15.982),
    c("Belgrade", "Serbia", 44.787, 20.457),
    c("Sarajevo", "Bosnia and Herzegovina", 43.856, 18.413),
    c("Podgorica", "Montenegro", 42.441, 19.263),
    c("Skopje", "North Macedonia", 41.998, 21.425),
    c("Tirana", "Albania", 41.328, 19.819),
    c("Pristina", "Kosovo", 42.663, 21.166),
    c("Sofia", "Bulgaria", 42.698, 23.322),
    c("Bucharest", "Romania", 44.427, 26.103),
    c("Chisinau", "Moldova", 47.011, 28.864),
    c("Kyiv", "Ukraine", 50.450, 30.523),
    c("Lviv", "Ukraine", 49.840, 24.030),
    c("Odesa", "Ukraine", 46.482, 30.723),
    c("Minsk", "Belarus", 53.904, 27.562),
    c("Athens", "Greece", 37.984, 23.728),
    c("Thessaloniki", "Greece", 40.640, 22.944),
    c("Valletta", "Malta", 35.899, 14.514),
    c("Moscow", "Russia", 55.756, 37.617),
    c("Saint Petersburg", "Russia", 59.939, 30.316),
    c("Murmansk", "Russia", 68.970, 33.075),
    // Asia
    c("Kabul", "Afghanistan", 34.555, 69.207),
    c("Islamabad", "Pakistan", 33.684, 73.048),
    c("Karachi", "Pakistan", 24.861, 67.010),
    c("Lahore", "Pakistan", 31.549, 74.344),
    c("New Delhi", "India", 28.614, 77.209),
    c("Mumbai", "India", 19.076, 72.878),
    c("Bengaluru", "India", 12.972, 77.595),
    c("Kolkata", "India", 22.573, 88.364),
    c("Chennai", "India", 13.083, 80.271),
    c("Hyderabad", "India", 17.385, 78.487),
    c("Dhaka", "Bangladesh", 23.811, 90.413),
    c("Kathmandu", "Nepal", 27.717, 85.324),
    c("Colombo", "Sri Lanka", 6.927, 79.861),
    c("Tashkent", "Uzbekistan", 41.299, 69.240),
    c("Astana", "Kazakhstan", 51.169, 71.449),
    c("Almaty", "Kazakhstan", 43.222, 76.851),
    c("Bishkek", "Kyrgyzstan", 42.875, 74.570),
    c("Dushanbe", "Tajikistan", 38.560, 68.774),
    c("Ashgabat", "Turkmenistan", 37.960, 58.326),
    c("Beijing", "China", 39.904, 116.407),
    c("Shanghai", "China", 31.230, 121.474),
    c("Guangzhou", "China", 23.129, 113.264),
    c("Shenzhen", "China", 22.543, 114.058),
    c("Chengdu", "China", 30.573, 104.066),
    c("Hong Kong", "China", 22.320, 114.169),
    c("Taipei", "Taiwan", 25.033, 121.565),
    c("Tokyo", "Japan", 35.676, 139.650),
    c("Osaka", "Japan", 34.694, 135.502),
    c("Sapporo", "Japan", 43.062, 141.354),
    c("Seoul", "South Korea", 37.567, 126.978),
    c("Busan", "South Korea", 35.180, 129.076),
    c("Pyongyang", "North Korea", 39.039, 125.763),
    c("Ulaanbaatar", "Mongolia", 47.886, 106.906),
    c("Bangkok", "Thailand", 13.756, 100.502),
    c("Hanoi", "Vietnam", 21.028, 105.854),
    c("Ho Chi Minh City", "Vietnam", 10.823, 106.630),
    c("Phnom Penh", "Cambodia", 11.556, 104.928),
    c("Vientiane", "Laos", 17.975, 102.633),
    c("Yangon", "Myanmar", 16.840, 96.173),
    c("Kuala Lumpur", "Malaysia", 3.139, 101.687),
    c("Singapore", "Singapore", 1.352, 103.820),
    c("Jakarta", "Indonesia", -6.208, 106.846),
    c("Surabaya", "Indonesia", -7.258, 112.752),
    c("Denpasar", "Indonesia", -8.670, 115.212),
    c("Manila", "Philippines", 14.600, 120.984),
    c("Novosibirsk", "Russia", 55.008, 82.935),
    c("Vladivostok", "Russia", 43.116, 131.886),
    // Africa
    c("Tunis", "Tunisia", 36.806, 10.182),
    c("Algiers", "Algeria", 36.754, 3.059),
    c("Rabat", "Morocco", 34.021, -6.841),
    c("Casablanca", "Morocco", 33.573, -7.590),
    c("Marrakesh", "Morocco", 31.629, -7.981),
    c("Tripoli", "Libya", 32.887, 13.191),
    c("Khartoum", "Sudan", 15.501, 32.560),
    c("Addis Ababa", "Ethiopia", 9.030, 38.740),
    c("Nairobi", "Kenya", -1.292, 36.822),
    c("Mombasa", "Kenya", -4.044, 39.668),
    c("Kampala", "Uganda", 0.348, 32.583),
    c("Kigali", "Rwanda", -1.944, 30.062),
    c("Dar es Salaam", "Tanzania", -6.792, 39.208),
    c("Mogadishu", "Somalia", 2.047, 45.318),
    c("Lagos", "Nigeria", 6.524, 3.379),
    c("Abuja", "Nigeria", 9.076, 7.399),
    c("Accra", "Ghana", 5.604, -0.187),
    c("Dakar", "Senegal", 14.716, -17.467),
    c("Abidjan", "Ivory Coast", 5.360, -4.008),
    c("Kinshasa", "DR Congo", -4.441, 15.266),
    c("Luanda", "Angola", -8.839, 13.289),
    c("Harare", "Zimbabwe", -17.829, 31.053),
    c("Lusaka", "Zambia", -15.387, 28.322),
    c("Maputo", "Mozambique", -25.969, 32.573),
    c("Johannesburg", "South Africa", -26.204, 28.047),
    c("Pretoria", "South Africa", -25.748, 28.229),
    c("Cape Town", "South Africa", -33.925, 18.424),
    c("Durban", "South Africa", -29.858, 31.022),
    c("Antananarivo", "Madagascar", -18.879, 47.508),
    // Americas
    c("New York", "United States", 40.713, -74.006),
    c("Washington", "United States", 38.907, -77.037),
    c("Boston", "United States", 42.360, -71.059),
    c("Chicago", "United States", 41.878, -87.630),
    c("Detroit", "United States", 42.331, -83.046),
    c("Nashville", "United States", 36.163, -86.781),
    c("Atlanta", "United States", 33.749, -84.388),
    c("Miami", "United States", 25.762, -80.192),
    c("Houston", "United States", 29.760, -95.370),
    c("Dallas", "United States", 32.777, -96.797),
    c("Denver", "United States", 39.739, -104.990),
    c("Phoenix", "United States", 33.448, -112.074),
    c("Los Angeles", "United States", 34.052, -118.244),
    c("San Francisco", "United States", 37.775, -122.419),
    c("Seattle", "United States", 47.606, -122.332),
    c("Anchorage", "United States", 61.218, -149.900),
    c("Honolulu", "United States", 21.307, -157.858),
    c("Toronto", "Canada", 43.653, -79.383),
    c("Montreal", "Canada", 45.502, -73.567),
    c("Ottawa", "Canada", 45.421, -75.697),
    c("Vancouver", "Canada", 49.283, -123.121),
    c("Calgary", "Canada", 51.045, -114.072),
    c("Mexico City", "Mexico", 19.433, -99.133),
    c("Guadalajara", "Mexico", 20.659, -103.350),
    c("Monterrey", "Mexico", 25.687, -100.316),
    c("Havana", "Cuba", 23.113, -82.366),
    c("Guatemala City", "Guatemala", 14.634, -90.507),
    c("San Jose", "Costa Rica", 9.928, -84.091),
    c("Panama City", "Panama", 8.983, -79.517),
    c("Bogota", "Colombia", 4.711, -74.072),
    c("Caracas", "Venezuela", 10.481, -66.904),
    c("Quito", "Ecuador", -0.181, -78.468),
    c("Lima", "Peru", -12.046, -77.043),
    c("La Paz", "Bolivia", -16.490, -68.119),
    c("Santiago", "Chile", -33.449, -70.669),
    c("Buenos Aires", "Argentina", -34.604, -58.382),
    c("Montevideo", "Uruguay", -34.901, -56.165),
    c("Asuncion", "Paraguay", -25.264, -57.576),
    c("Sao Paulo", "Brazil", -23.551, -46.633),
    c("Rio de Janeiro", "Brazil", -22.907, -43.173),
    c("Brasilia", "Brazil", -15.794, -47.882),
    // Oceania
    c("Sydney", "Australia", -33.869, 151.209),
    c("Melbourne", "Australia", -37.814, 144.963),
    c("Brisbane", "Australia", -27.470, 153.026),
    c("Perth", "Australia", -31.951, 115.861),
    c("Adelaide", "Australia", -34.929, 138.601),
    c("Canberra", "Australia", -35.281, 149.130),
    c("Darwin", "Australia", -12.463, 130.842),
    c("Hobart", "Australia", -42.882, 147.327),
    c("Auckland", "New Zealand", -36.849, 174.763),
    c("Wellington", "New Zealand", -41.287, 174.776),
    c("Christchurch", "New Zealand", -43.532, 172.637),
    c("Suva", "Fiji", -18.124, 178.450),
    c("Port Moresby", "Papua New Guinea", -9.443, 147.180),
];

/// Lower case, with a few common accented letters folded to plain ones
/// ("Bogotá" finds "Bogota").
fn fold(text: &str) -> String {
    text.trim()
        .chars()
        .flat_map(|ch| ch.to_lowercase())
        .map(|ch| match ch {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            'ş' => 's',
            'ğ' => 'g',
            'ı' => 'i',
            '-' => ' ',
            other => other,
        })
        .collect()
}

/// Cities whose name or country starts with (or contains) the query, best first:
/// names that start with it, then names that contain it, then cities whose
/// country matches. Within a group the list order is kept.
pub fn search(query: &str) -> Vec<&'static City> {
    let q = fold(query);
    if q.is_empty() {
        return Vec::new();
    }
    let mut ranked: Vec<(u8, usize, &'static City)> = CITIES
        .iter()
        .enumerate()
        .filter_map(|(i, city)| {
            let name = fold(city.name);
            let country = fold(city.country);
            let rank = if name.starts_with(&q) {
                0
            } else if name.contains(&q) {
                1
            } else if country.starts_with(&q) {
                2
            } else if country.contains(&q) {
                3
            } else {
                return None;
            };
            Some((rank, i, city))
        })
        .collect();
    ranked.sort_by_key(|&(rank, i, _)| (rank, i));
    ranked.into_iter().map(|(_, _, city)| city).collect()
}

/// The city with exactly this name (case-insensitive), if listed.
pub fn find(name: &str) -> Option<&'static City> {
    let n = fold(name);
    CITIES.iter().find(|c| fold(c.name) == n)
}

/// One coordinate: a number with an optional hemisphere letter before or after.
fn coord(token: &str) -> Option<(f64, Option<char>)> {
    let t = token.trim();
    let first = t.chars().next()?;
    let last = t.chars().last()?;
    let (num, hemi) = if "NSEW".contains(last) && t.len() > 1 {
        (&t[..t.len() - 1], Some(last))
    } else if "NSEW".contains(first) && t.len() > 1 {
        (&t[1..], Some(first))
    } else {
        (t, None)
    };
    let v: f64 = num.trim().parse().ok()?;
    if !v.is_finite() {
        return None;
    }
    match hemi {
        Some('S') | Some('W') => {
            if v < 0.0 {
                None
            } else {
                Some((-v, hemi))
            }
        }
        Some(_) if v < 0.0 => None,
        _ => Some((v, hemi)),
    }
}

/// Parses typed coordinates such as "36.19, 44.01" or "36.19N 44.01E".
/// Without letters the first number is the latitude. Returns (lat, lon)
/// in degrees, north and east positive, or `None` if out of range.
pub fn parse_coords(text: &str) -> Option<(f64, f64)> {
    let cleaned: String = text
        .to_uppercase()
        .chars()
        .map(|ch| match ch {
            ',' | ';' | '/' | '\t' => ' ',
            '°' => ' ',
            other => other,
        })
        .collect();
    // Join lone hemisphere letters to the number before them ("36.19 N").
    let mut tokens: Vec<String> = Vec::new();
    for word in cleaned.split_whitespace() {
        if word.len() == 1 && "NSEW".contains(word) {
            match tokens.last_mut() {
                Some(prev) if prev.chars().last().is_some_and(|c| c.is_ascii_digit()) => {
                    prev.push_str(word)
                }
                _ => tokens.push(word.to_string()),
            }
        } else {
            tokens.push(word.to_string());
        }
    }
    if tokens.len() != 2 {
        return None;
    }
    let a = coord(&tokens[0])?;
    let b = coord(&tokens[1])?;
    let is_lat = |h: Option<char>| matches!(h, Some('N') | Some('S'));
    let is_lon = |h: Option<char>| matches!(h, Some('E') | Some('W'));
    let (lat, lon) = if is_lon(a.1) || is_lat(b.1) {
        if is_lon(b.1) || is_lat(a.1) {
            return None; // both latitudes or both longitudes
        }
        (b.0, a.0)
    } else {
        (a.0, b.0)
    };
    if (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon) {
        Some((lat, lon))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_is_sane() {
        assert!(CITIES.len() >= 200, "{}", CITIES.len());
        for c in CITIES {
            assert!((-90.0..=90.0).contains(&c.lat), "{}", c.name);
            assert!((-180.0..=180.0).contains(&c.lon), "{}", c.name);
            assert!(!c.name.is_empty() && !c.country.is_empty());
        }
        // No duplicate (name, country) pairs.
        for (i, a) in CITIES.iter().enumerate() {
            for b in &CITIES[i + 1..] {
                assert!(!(a.name == b.name && a.country == b.country), "{}", a.name);
            }
        }
        for name in [
            "Erbil",
            "Sulaymaniyah",
            "Duhok",
            "Kirkuk",
            "Halabja",
            "Zakho",
            "Baghdad",
            "Basra",
            "Mosul",
            "Najaf",
        ] {
            assert_eq!(find(name).unwrap().country, "Iraq", "{name}");
        }
    }

    #[test]
    fn search_ranks_prefix_then_contains_then_country() {
        assert!(search("").is_empty());
        assert!(search("   ").is_empty());
        assert_eq!(search("erbil")[0].name, "Erbil");
        assert_eq!(search("  ERB ")[0].name, "Erbil");
        // Names starting with "san" (Sanandaj, San Jose ...) before names
        // containing it (Busan).
        let r = search("san");
        let first_contains = r
            .iter()
            .position(|c| !fold(c.name).starts_with("san"))
            .unwrap();
        assert!(first_contains >= 3);
        assert!(r[..first_contains]
            .iter()
            .all(|c| fold(c.name).starts_with("san")));
        assert!(r[first_contains..]
            .iter()
            .all(|c| !fold(c.name).starts_with("san")));
        assert!(r.iter().any(|c| c.name == "Busan"));
        // Country matches come after name matches.
        let r = search("iraq");
        assert!(r.len() >= 10);
        assert!(r.iter().all(|c| c.country == "Iraq"));
        // Prefix matches in list order, then names containing the query.
        let r = search("van");
        assert_eq!(r[0].name, "Van");
        assert_eq!(r[1].name, "Vancouver");
        assert_eq!(r[2].name, "Yerevan");
        // "united": only country matches, kept in list order.
        let r = search("united");
        assert_eq!(r[0].name, "Abu Dhabi");
        assert!(r.iter().all(|c| c.country.starts_with("United")));
        let r = search("tun");
        assert_eq!(r[0].name, "Tunis");
        assert!(search("zzzz").is_empty());
        assert_eq!(search("bogotá")[0].name, "Bogota");
        assert_eq!(search("ho chi")[0].name, "Ho Chi Minh City");
        assert_eq!(search("Saint-Petersburg")[0].name, "Saint Petersburg");
    }

    #[test]
    fn coordinates() {
        assert_eq!(parse_coords("36.19, 44.01"), Some((36.19, 44.01)));
        assert_eq!(parse_coords("36.19 44.01"), Some((36.19, 44.01)));
        assert_eq!(parse_coords("36.19N 44.01E"), Some((36.19, 44.01)));
        assert_eq!(parse_coords("36.19 N, 44.01 E"), Some((36.19, 44.01)));
        assert_eq!(parse_coords("33.9S 151.2E"), Some((-33.9, 151.2)));
        assert_eq!(parse_coords("33.9s 151.2e"), Some((-33.9, 151.2)));
        assert_eq!(parse_coords("40.7N 74.0W"), Some((40.7, -74.0)));
        assert_eq!(parse_coords("-33.9, 151.2"), Some((-33.9, 151.2)));
        assert_eq!(parse_coords("44.01E 36.19N"), Some((36.19, 44.01)));
        assert_eq!(parse_coords("36.19° N 44.01° E"), Some((36.19, 44.01)));
        assert_eq!(parse_coords("N36.19 E44.01"), Some((36.19, 44.01)));
        assert_eq!(parse_coords("90, 180"), Some((90.0, 180.0)));
        // Out of range or malformed.
        assert_eq!(parse_coords("91, 44"), None);
        assert_eq!(parse_coords("36, 181"), None);
        assert_eq!(parse_coords("36N 44N"), None);
        assert_eq!(parse_coords("36E 44E"), None);
        assert_eq!(parse_coords("-36S 44E"), None);
        assert_eq!(parse_coords("36.19"), None);
        assert_eq!(parse_coords("1 2 3"), None);
        assert_eq!(parse_coords("abc, def"), None);
        assert_eq!(parse_coords("NaN, 4"), None);
        assert_eq!(parse_coords("inf, 4"), None);
        assert_eq!(parse_coords(""), None);
    }
}
