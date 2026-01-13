use std::{collections::HashSet, fs, process::ExitCode};

use skrifa::{FontRef, GlyphId, MetadataProvider, Tag, instance::Location, raw::TableProvider};
use sleipnir::{
    draw_glyph::DrawOptions,
    icon2svg::draw_icon,
    iconid::{Icon, IconIdentifier, Icons},
    pathstyle::SvgPathStyle,
    text2png::text2png,
};
use tiny_skia::Color;

fn print_problems(desc: &str, offenders: &[Icon]) {
    for offender in offenders {
        println!("{desc} {}", offender.names.join(","))
    }
}

trait PrintOnly {
    fn print_only(&self, desc: &str, other: &Self) -> usize;
}

impl PrintOnly for HashSet<Icon> {
    fn print_only(&self, desc: &str, other: &Self) -> usize {
        let mut only = self.difference(&other).cloned().collect::<Vec<_>>();
        only.sort_by_cached_key(|i| i.names.join(","));
        print_problems(desc, &only);
        only.len()
    }
}

fn axes(font: &FontRef<'_>) -> Vec<Axis> {
    let fvar = font.fvar().unwrap();
    let mut axes = Vec::new();
    for raw_axis in fvar.axes().unwrap() {
        axes.push(Axis {
            tag: raw_axis.axis_tag(),
            min: raw_axis.min_value().to_i32(),
            default: raw_axis.default_value().to_i32(),
            max: raw_axis.max_value().to_i32(),
        });
    }
    axes
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Axis {
    tag: Tag,
    min: i32,
    default: i32,
    max: i32,
}

fn stops(min: i32, max: i32, step: i32) -> Vec<i32> {
    let mut stops = Vec::new();
    let mut curr = min;
    while curr <= max {
        stops.push(curr);
        curr += step
    }
    stops.push(max);
    stops
}

impl Axis {
    fn stops(&self) -> Vec<(Tag, i32)> {
        const FILL_AXIS: Tag = Tag::new(b"FILL");

        let mut values = match self.tag {
            FILL_AXIS => stops(self.min, self.max, 1),
            _ => vec![self.min, self.default, self.max],
        };
        values.dedup();
        values.into_iter().map(|v| (self.tag.clone(), v)).collect()
    }
}

fn constellation(font: &FontRef<'_>) -> HashSet<Location> {
    let axes = axes(font);
    let mut stop_lists = axes.iter().map(|a| a.stops()).collect::<Vec<_>>();

    let mut raw_locations = vec![Vec::<(Tag, f32)>::new()];

    while let Some(stops) = stop_lists.pop() {
        let mut new_locations = Vec::new();

        for location in raw_locations.iter() {
            for (tag, pos) in stops.iter() {
                let mut location = location.clone();
                location.push((tag.clone(), *pos as f32));
                new_locations.push(location);
            }
        }

        raw_locations = new_locations;
    }

    let font_axes = font.axes();

    raw_locations
        .into_iter()
        .map(|l| font_axes.location(&l))
        .collect()
}

fn save_failure(icon_name: &str, side: &str, content: &str, nth: usize) {
    let path = format!("/tmp/failure.{icon_name}.{side}.{nth}.svg");
    fs::write(&path, content).unwrap_or_else(|e| panic!("Unable to write {path}: {e}"));
}

fn save_png_failure(icon_name: &str, side: &str, content: &[u8], nth: usize) {
    let path = format!("/tmp/failure.{icon_name}.{side}.{nth}.png");
    fs::write(&path, content).unwrap_or_else(|e| panic!("Unable to write {path}: {e}"));
}

fn main() -> ExitCode {
    let paths = std::env::args().skip(1).collect::<Vec<_>>();
    if paths.len() != 2 {
        println!("Pass the two font files in");
        return ExitCode::FAILURE;
    }

    let mut raws = Vec::new();
    for path in paths.iter() {
        raws.push(fs::read(path).unwrap_or_else(|e| panic!("Unable to read {}: {e}", path)));
    }

    let mut refs = Vec::new();
    for (raw, path) in raws.iter().zip(paths.iter()) {
        refs.push(
            FontRef::new(raw)
                .unwrap_or_else(|e| panic!("Unable to create font ref to {}: {e}", path)),
        );
    }

    let upem = refs
        .iter()
        .map(|f| f.head().unwrap().units_per_em())
        .max()
        .unwrap();

    let mut icons = Vec::new();
    for (font_ref, path) in refs.iter().zip(paths.iter()) {
        icons.push(
            font_ref
                .icons()
                .unwrap_or_else(|e| panic!("Unable to enumerate icons from {}: {e}", path))
                .into_iter()
                .map(|i| {
                    let mut i = Icon {
                        gid: GlyphId::new(0),
                        ..i
                    };
                    i.names.sort();
                    i.codepoints.sort();
                    i
                })
                .collect::<HashSet<_>>(),
        );
    }

    let constellations = refs.iter().map(constellation).collect::<Vec<_>>();

    let [left_locs, right_locs] = constellations.as_slice() else {
        unreachable!("Eh?");
    };

    if left_locs != right_locs {
        println!("Inconsistent location sets, did axes or ranges of axes change?");
    }
    let test_locs = left_locs.intersection(right_locs).collect::<HashSet<_>>();

    let [left_icons, right_icons] = icons.as_slice() else {
        unreachable!("What?");
    };
    let mut test_icons = left_icons.intersection(right_icons).collect::<Vec<_>>();
    test_icons.sort_by_cached_key(|i| i.names.join(","));

    println!(
        "Testing {} icons at {} locations at {upem}x{upem}...",
        test_icons.len(),
        test_locs.len()
    );

    let mut errs = 0;

    errs += left_icons.print_only("only_left", &right_icons);
    errs += right_icons.print_only("only_right", &left_icons);

    for icon in test_icons {
        let mut bad_locs = Vec::new();
        for loc in test_locs.iter() {
            // let draw_opts = DrawOptions::new(
            //     IconIdentifier::Name(icon.names[0].as_str().into()),
            //     upem.into(),
            //     (*loc).into(),
            //     SvgPathStyle::Unchanged(0),
            // );
            // let mut svgs = Vec::new();
            // for font_ref in refs.iter() {
            //     svgs.push(
            //         draw_icon(font_ref, &draw_opts)
            //             .unwrap_or_else(|e| panic!("Unable to draw {icon:?} at {loc:?}: {e}")),
            //     );
            // }
            // let [left_svg, right_svg] = svgs.as_slice() else {
            //     unreachable!("??");
            // };
            // if left_svg != right_svg {
            //     save_failure(icon.names[0].as_str(), "left", &left_svg, bad_locs.len());
            //     save_failure(icon.names[0].as_str(), "right", &right_svg, bad_locs.len());

            //     bad_locs.push(loc);
            // }
            let mut pngs = Vec::new();
            for raw_font in raws.iter() {
                pngs.push(
                    text2png(
                        icon.names[0].as_str(),
                        64.0,
                        1.0,
                        raw_font,
                        Color::BLACK,
                        Color::WHITE,
                    )
                    .unwrap_or_else(|e| panic!("Unable to draw {icon:?} at {loc:?}: {e}")),
                );
            }
            let [left_png, right_png] = pngs.as_slice() else {
                unreachable!("Huh?");
            };
            if left_png != right_png {
                save_png_failure(icon.names[0].as_str(), "left", &left_png, bad_locs.len());
                save_png_failure(icon.names[0].as_str(), "right", &right_png, bad_locs.len());
                bad_locs.push(loc);
            }
        }
        errs += bad_locs.len();
        if !bad_locs.is_empty() {
            println!(
                "{} fails at {}/{} locations",
                icon.names[0],
                bad_locs.len(),
                test_locs.len()
            );
        } else {
            println!("{} passes", icon.names[0]);
        }
    }

    if errs == 0 {
        ExitCode::SUCCESS
    } else {
        println!("Eeek, {errs} failures!");
        ExitCode::FAILURE
    }
}
