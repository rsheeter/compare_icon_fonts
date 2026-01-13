use std::{collections::HashSet, fs, process::ExitCode};

use clap::Parser;
use kurbo::BezPath;
use regex::Regex;
use skrifa::{FontRef, GlyphId, MetadataProvider, Tag, instance::Location, raw::TableProvider};
use sleipnir::{
    draw_glyph::DrawOptions,
    icon2svg::draw_icon,
    iconid::{Icon, IconIdentifier, Icons},
    pathstyle::SvgPathStyle,
};

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

fn stops(min: i32, default: i32, max: i32, step: i32) -> Vec<i32> {
    let mut stops = Vec::new();
    let mut curr = min;
    while curr <= max {
        stops.push(curr);
        curr += step
    }
    stops.push(default);
    stops.push(max);
    stops
}

impl Axis {
    fn stops(&self) -> Vec<(Tag, i32)> {
        const FILL_AXIS: Tag = Tag::new(b"FILL");
        const GRADE_AXIS: Tag = Tag::new(b"GRAD");
        const ROUND_AXIS: Tag = Tag::new(b"ROND");
        const OPSZ_AXIS: Tag = Tag::new(b"opsz");
        const WGHT_AXIS: Tag = Tag::new(b"wght");

        let mut values = match self.tag {
            FILL_AXIS => stops(self.min, self.default, self.max, 1),
            GRADE_AXIS => stops(self.min, self.default, self.max, 25),
            ROUND_AXIS => stops(self.min, self.default, self.max, 50),
            OPSZ_AXIS => stops(self.min, self.default, self.max, 16),
            WGHT_AXIS => stops(self.min, self.default, self.max, 200),
            _ => panic!("What is {}?!", self.tag),
        };
        values.sort();
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

fn subpaths(icon_name: &str, path: &str) -> Vec<BezPath> {
    path.chars()
        .enumerate()
        .filter_map(|(i, c)| {
            if ['m', 'M'].contains(&c) {
                Some(i)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .windows(2)
        .map(|w| &path[w[0]..w[1]])
        .map(|s| {
            BezPath::from_svg(s).unwrap_or_else(|e| panic!("Invalid path for {icon_name}: {e}"))
        })
        .collect()
}

fn parse_path(svg: &str) -> (&str, &str, &str) {
    let preamble = "<path d=\"";
    let idx = svg.find(preamble).unwrap() + preamble.len();
    let (preamble, rest) = svg.split_at(idx);
    let idx = rest.find("\"").unwrap();
    let (path, suffix) = rest.split_at(idx);
    (preamble, path, suffix)
}

fn equivalent_paths(icon_name: &str, left_svg: &str, right_svg: &str) -> bool {
    let left_path = parse_path(left_svg).1;
    let right_path = parse_path(right_svg).1;

    let left_subpaths = subpaths(icon_name, left_path);
    let right_subpaths = subpaths(icon_name, right_path);

    if left_subpaths.len() != right_subpaths.len() {
        return false;
    }

    left_subpaths
        .into_iter()
        .zip(right_subpaths.into_iter())
        .all(|(left_subpath, right_subpath)| {
            if left_subpath == right_subpath {
                return true;
            }
            if left_subpath.is_empty() {
                return false;
            }
            // Sometimes one is rotated
            let left_elements = left_subpath.into_elements();
            let mut right_elements = right_subpath.into_elements();
            right_elements.rotate_right(1);

            left_elements == right_elements
        })
}

fn save_failure(icon_name: &str, side: &str, content: &str, nth: usize) {
    // reformat the svg slightly
    let content = content
        .replace("<path", "\n  <path")
        .replace("</svg>", "\n</svg>");

    let (preamble, svg_path, suffix) = parse_path(&content);

    let mut formatted = preamble.to_string() + "\n";

    let cmd_indices = svg_path
        .chars()
        .enumerate()
        .filter_map(|(i, c)| if c.is_alphabetic() { Some(i) } else { None })
        .collect::<Vec<_>>();
    for cmd in cmd_indices.windows(2).map(|w| &svg_path[w[0]..w[1]]) {
        formatted += cmd;
        formatted += "\n";
    }

    formatted += suffix;

    let path = format!("/tmp/failure.{icon_name}.{side}.{nth}.svg");
    fs::write(&path, formatted).unwrap_or_else(|e| panic!("Unable to write {path}: {e}"));

    let path = format!("/tmp/failure.{icon_name}.{side}.{nth}.segments");
    let mut segments = String::new();
    for subpath in subpaths(icon_name, &svg_path) {
        for seg in subpath.segments() {
            segments += format!("{seg:?}\n").as_str();
        }
        segments += "\n";
    }
    fs::write(&path, segments).unwrap_or_else(|e| panic!("Unable to write {path}: {e}"));
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Regex filter for icon names
    #[arg(short, long, default_value = None)]
    filter: Option<String>,

    /// Number of times to greet
    #[arg(num_args = 2)]
    paths: Vec<String>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    if args.paths.len() != 2 {
        println!("Pass the two font files in");
        return ExitCode::FAILURE;
    }

    let mut raws = Vec::new();
    for path in args.paths.iter() {
        raws.push(fs::read(path).unwrap_or_else(|e| panic!("Unable to read {}: {e}", path)));
    }

    let mut refs = Vec::new();
    for (raw, path) in raws.iter().zip(args.paths.iter()) {
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

    let filter = args
        .filter
        .map(|raw| Regex::new(&raw).unwrap_or_else(|e| panic!("Invalid filter: {e}")));

    let mut icons = Vec::new();
    for (font_ref, path) in refs.iter().zip(args.paths.iter()) {
        icons.push(
            font_ref
                .icons()
                .unwrap_or_else(|e| panic!("Unable to enumerate icons from {}: {e}", path))
                .into_iter()
                .filter(|i| {
                    let Some(filter) = filter.as_ref() else {
                        return true;
                    };
                    i.names.iter().any(|n| filter.find(n).is_some())
                })
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
        "Testing {} icons at {} locations...",
        test_icons.len(),
        test_locs.len()
    );

    let mut errs = 0;

    errs += left_icons.print_only("only_left", &right_icons);
    errs += right_icons.print_only("only_right", &left_icons);

    errs += test_icons
        .iter()
        .map(|icon| {
            let mut bad_locs = Vec::new();
            let mut good_locs = Vec::new();
            for loc in test_locs.iter() {
                let draw_opts = DrawOptions::new(
                    IconIdentifier::Name(icon.names[0].as_str().into()),
                    upem.into(),
                    (*loc).into(),
                    SvgPathStyle::Unchanged(0),
                );
                let mut svgs = Vec::new();
                for font_ref in refs.iter() {
                    svgs.push(
                        draw_icon(font_ref, &draw_opts)
                            .unwrap_or_else(|e| panic!("Unable to draw {icon:?} at {loc:?}: {e}")),
                    );
                }
                let [left_svg, right_svg] = svgs.as_slice() else {
                    unreachable!("??");
                };
                if !equivalent_paths(icon.names[0].as_str(), left_svg, right_svg) {
                    save_failure(icon.names[0].as_str(), "left", &left_svg, bad_locs.len());
                    save_failure(icon.names[0].as_str(), "right", &right_svg, bad_locs.len());

                    bad_locs.push(loc);
                } else {
                    good_locs.push(loc);
                }
            }
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
            bad_locs.len()
        })
        .sum::<usize>();

    if errs == 0 {
        ExitCode::SUCCESS
    } else {
        println!("Eeek, {errs} failures!");
        ExitCode::FAILURE
    }
}
