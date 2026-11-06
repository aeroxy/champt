//! Map geometry: where a pin may go, and which pins a viewport covers.

/// Grid that pins snap to: 0.001° is about 110 m north–south, and less
/// east–west away from the equator. Coarse enough that a thread started at
/// home does not mark the front door.
const GRID: f64 = 1000.0;

/// Web Mercator stops near ±85°, so the map cannot show a pin beyond it.
const MAX_LAT: f64 = 85.0;

/// Snaps a position to the pin grid, or says why it can't be pinned.
pub fn snap(lat: f64, lng: f64) -> Result<(f64, f64), &'static str> {
    if !lat.is_finite() || !lng.is_finite() || lat.abs() > MAX_LAT || lng.abs() > 180.0 {
        return Err("That location is outside the map.");
    }
    let round = |x: f64| (x * GRID).round() / GRID;
    Ok((round(lat), round(wrap(lng))))
}

/// Brings any longitude into [-180, 180). One already in range is returned
/// untouched: the arithmetic would nudge 2.2 to 2.1999999999999886.
fn wrap(lng: f64) -> f64 {
    if (-180.0..180.0).contains(&lng) {
        return lng;
    }
    (lng + 180.0).rem_euclid(360.0) - 180.0
}

/// A viewport in the order the map gives it: `west,south,east,north`.
///
/// Once the map is zoomed out far enough to repeat the world, west and east
/// run past ±180, and a view across the antimeridian ends up with west east
/// of east after wrapping. `wraps` marks that case, which needs `lng >= west
/// OR lng <= east` instead of a plain range.
#[derive(Debug, PartialEq)]
pub struct BBox {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
    pub wraps: bool,
}

impl BBox {
    pub fn parse(text: &str) -> Result<BBox, &'static str> {
        const USAGE: &str = "bbox must be west,south,east,north in degrees.";
        let parts: Vec<f64> = text
            .split(',')
            .map(|part| part.trim().parse::<f64>())
            .collect::<Result<_, _>>()
            .map_err(|_| USAGE)?;
        let [west, south, east, north] = parts[..] else {
            return Err(USAGE);
        };
        if parts.iter().any(|x| !x.is_finite()) || south > north {
            return Err(USAGE);
        }
        let (south, north) = (south.max(-90.0), north.min(90.0));
        if east - west >= 360.0 {
            return Ok(BBox { west: -180.0, south, east: 180.0, north, wraps: false });
        }
        let (west, east) = (wrap(west), wrap(east));
        Ok(BBox { west, south, east, north, wraps: west > east })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snaps_to_the_grid() {
        assert_eq!(snap(48.858_37, 2.294_48), Ok((48.858, 2.294)));
        assert_eq!(snap(-33.856_78, 151.215_29), Ok((-33.857, 151.215)));
    }

    #[test]
    fn snaps_longitude_into_range() {
        assert_eq!(snap(0.0, 180.0), Ok((0.0, -180.0)));
    }

    #[test]
    fn refuses_positions_off_the_map() {
        assert!(snap(86.0, 0.0).is_err());
        assert!(snap(0.0, 181.0).is_err());
        assert!(snap(f64::NAN, 0.0).is_err());
    }

    #[test]
    fn parses_a_plain_viewport() {
        let b = BBox::parse("2.2,48.8,2.4,48.9").unwrap();
        assert_eq!(b, BBox { west: 2.2, south: 48.8, east: 2.4, north: 48.9, wraps: false });
    }

    #[test]
    fn a_view_across_the_antimeridian_wraps() {
        // The map reports this view as 170..190.
        let b = BBox::parse("170,-10,190,10").unwrap();
        assert!(b.wraps);
        assert_eq!((b.west, b.east), (170.0, -170.0));
        // ...and as -190..-170 when panned the other way.
        let b = BBox::parse("-190,-10,-170,10").unwrap();
        assert!(b.wraps);
        assert_eq!((b.west, b.east), (170.0, -170.0));
    }

    #[test]
    fn a_view_of_the_whole_world_covers_every_longitude() {
        let b = BBox::parse("-400,-80,400,80").unwrap();
        assert_eq!((b.west, b.east, b.wraps), (-180.0, 180.0, false));
    }

    #[test]
    fn rejects_malformed_viewports() {
        assert!(BBox::parse("1,2,3").is_err());
        assert!(BBox::parse("a,b,c,d").is_err());
        assert!(BBox::parse("0,10,1,5").is_err()); // south above north
    }
}
