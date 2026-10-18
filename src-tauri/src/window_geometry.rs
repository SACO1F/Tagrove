//! Geometry uses physical monitor work areas and saved physical positions;
//! requested sizes remain logical, so the chosen monitor controls their scale.
#[derive(Clone, Copy, Debug)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}
#[derive(Debug, PartialEq)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub width: f64,
    pub height: f64,
}
/// Native pixel-to-logical conversion may yield 279.2 at a 280 logical-pixel
/// minimum on fractional DPI. Normalize only native geometry before persisting;
/// the user-facing settings validator remains strict.
pub fn expanded_size(width: f64, height: f64) -> (u32, u32) {
    let width = if width.is_finite() { width } else { 340. };
    let height = if height.is_finite() { height } else { 460. };
    (width.round().clamp(280., 900.) as u32,
     height.round().clamp(320., 1000.) as u32)
}
pub fn fit(
    areas: &[Area],
    saved: Option<(i32, i32)>,
    width: f64,
    height: f64,
) -> Option<Placement> {
    fit_mode(areas, saved, width, height, false)
}
/// Collapse/expand changes height only. Native undecorated minimum widths can
/// be below the saved logical minimum because of Windows frame offsets.
pub fn transition(
    areas: &[Area], saved: Option<(i32, i32)>, width: f64, height: f64,
) -> Option<Placement> {
    fit_mode(areas, saved, width, height, true)
}
fn fit_mode(
    areas: &[Area], saved: Option<(i32, i32)>, width: f64, height: f64,
    preserve_width: bool,
) -> Option<Placement> {
    let first = areas.first()?;
    let chosen = saved
        .and_then(|(x, y)| {
            areas.iter().find(|a| {
                x as i64 >= a.x as i64
                    && y as i64 >= a.y as i64
                    && (x as i64) < a.x as i64 + a.width as i64
                    && (y as i64) < a.y as i64 + a.height as i64
            })
        })
        .unwrap_or(first);
    let scale = if chosen.scale.is_finite() && chosen.scale > 0. {
        chosen.scale
    } else {
        1.
    };
    let width = if preserve_width {
        if width.is_finite() && width > 0. { width } else { 340. }
    } else {
        width.clamp(280., 900.)
            .min((chosen.width as f64 / scale - 16.).max(280.))
    };
    let min_height = if height < 100. { 64. } else { 320. };
    let height = height
        .clamp(min_height, 1000.)
        .min((chosen.height as f64 / scale - 16.).max(min_height));
    let left = chosen.x as i64 + 8;
    let top = chosen.y as i64 + 8;
    let right =
        (chosen.x as i64 + chosen.width as i64 - (width * scale).ceil() as i64 - 8).max(left);
    let bottom =
        (chosen.y as i64 + chosen.height as i64 - (height * scale).ceil() as i64 - 8).max(top);
    let (x, y) = saved
        .map(|(x, y)| (x as i64, y as i64))
        .unwrap_or((right, top + 40));
    Some(Placement {
        x: x.clamp(left, right) as i32,
        y: y.clamp(top, bottom) as i32,
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fractional_dpi_native_minimum_stays_in_settings_range() {
        for scale in [1., 1.25, 1.5, 1.75, 2.] {
            let width = ((280_f64 * scale).floor() - 1.) / scale;
            let height = ((320_f64 * scale).floor() - 1.) / scale;
            assert_eq!(expanded_size(width, height), (280, 320));
        }
        assert_eq!(expanded_size(901.,1001.),(900,1000));
        assert_eq!(expanded_size(f64::NAN,f64::INFINITY),(340,460));
    }
    const MAIN: Area = Area {
        x: 0,
        y: 0,
        width: 1920,
        height: 1040,
        scale: 1.,
    };
    #[test]
    fn restores_on_negative_monitor_and_scales_size() {
        let second = Area {
            x: -1600,
            y: 0,
            width: 1600,
            height: 860,
            scale: 2.,
        };
        let fitted = fit(&[MAIN, second], Some((-1000, 120)), 900., 1000.).unwrap();
        assert_eq!(fitted.width, 784.);
        assert_eq!(fitted.height, 414.);
        assert!(fitted.x >= -1600 && fitted.x + (fitted.width * 2.) as i32 <= 0);
        assert_eq!(fitted.y, 24);
    }
    #[test]
    fn removed_monitor_and_offscreen_position_return_to_primary() {
        for saved in [Some((5000, 300)), Some((-4000, -100)), None] {
            let p = fit(&[MAIN], saved, 340., 460.).unwrap();
            assert!((8..=1572).contains(&p.x));
            assert!((8..=572).contains(&p.y));
        }
        assert_eq!(fit(&[MAIN], Some((120, 160)), 340., 460.).unwrap().x, 120);
    }
    #[test]
    fn collapse_transition_preserves_live_width_at_fractional_dpi() {
        for scale in [1., 1.25, 1.5, 1.75, 2.] {
            let area = Area { scale, ..MAIN };
            for pixels in [(264. * scale) as u32, (280. * scale) as u32 - 1, (340. * scale) as u32] {
                let width = pixels as f64 / scale;
                let compact = transition(&[area], Some((100,100)), width, 64.).unwrap();
                let expanded = transition(&[area], Some((compact.x,compact.y)), compact.width, 460.).unwrap();
                assert_eq!(compact.width, width);
                assert_eq!(expanded.width, width);
                assert_eq!((compact.width * scale).round() as u32, pixels);
                assert_eq!(compact.height, 64.);
                assert_eq!(expanded.height, 460.);
                assert_eq!(compact.x, 100);
            }
        }
        assert_eq!(fit(&[MAIN], Some((100,100)), 264., 320.).unwrap().width, 280.);
        let compact = transition(&[MAIN], Some((1900,1030)), 264., 64.).unwrap();
        assert!(compact.x + 264 <= 1912 && compact.y + 64 <= 1032);
    }
    #[test]
    fn compact_and_small_workareas_keep_title_reachable() {
        let area = Area {
            x: 0,
            y: -300,
            width: 200,
            height: 200,
            scale: 1.5,
        };
        let p = fit(&[area], Some((100, -150)), 340., 64.).unwrap();
        assert_eq!(p.x, 8);
        assert!(p.y >= -292 && p.y <= -204);
        assert_eq!(p.height, 64.);
        assert!(fit(&[], None, 340., 460.).is_none());
    }
}
