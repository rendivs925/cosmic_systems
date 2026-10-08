//! Pure geometry for the procedural launch service tower.
//!
//! The tower is presentation-only: it never contributes collision, terrain
//! height, or forces. Keeping its member endpoints here, rather than inline in
//! the spawn system, lets tests assert that every brace and arm actually meets a
//! column or platform. The previous inline version centred the horizontal braces
//! on the tower axis between the columns and started the service arms in the
//! unsupported interior, so members visibly floated.
//!
//! Coordinates are local to the pad root: the vehicle stands near the origin,
//! the tower stands at positive +X (`center_x_m`), +Y is the pad up axis, and
//! +Z is the lateral axis. The tower is offset clear of the vehicle by
//! `center_x_m`, and every member endpoint lands on a column or platform edge.

/// Number of stacked tower platforms. The topmost platform supports the mast.
pub const TOWER_LEVEL_COUNT: usize = 5;
/// Half the spacing between adjacent columns, in meters.
pub const COLUMN_HALF_SPAN_M: f32 = 4.0;
/// Half the platform edge length, in meters. Must exceed the column span so the
/// platforms enclose the columns.
pub const PLATFORM_HALF_SPAN_M: f32 = 4.5;
/// Platform slab thickness, in meters.
pub const PLATFORM_THICKNESS_M: f32 = 0.35;
/// Default thickness of a beam member, in meters.
pub const MEMBER_THICKNESS_M: f32 = 0.28;

/// Bounded, validated tower dimensions derived from the vehicle size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TowerLayout {
    /// Column height and the level of the top platform, in meters.
    pub height_m: f32,
    /// Tower axis position on +X, in meters.
    pub center_x_m: f32,
    /// Mast tip height above the pad, in meters.
    pub mast_height_m: f32,
    /// Half span of the column footprint, in meters.
    pub column_half_span_m: f32,
    /// Half span of a platform slab, in meters.
    pub platform_half_span_m: f32,
}

/// A straight structural member between two attachment points, local to the pad
/// root. `thickness` is the cross-section width in meters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TowerMember {
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub thickness: f32,
}

impl TowerMember {
    fn new(start: [f32; 3], end: [f32; 3], thickness: f32) -> Self {
        Self {
            start,
            end,
            thickness,
        }
    }

    /// Midpoint of the member.
    pub fn midpoint(&self) -> [f32; 3] {
        [
            (self.start[0] + self.end[0]) * 0.5,
            (self.start[1] + self.end[1]) * 0.5,
            (self.start[2] + self.end[2]) * 0.5,
        ]
    }

    /// Euclidean length of the member, in meters.
    pub fn length(&self) -> f32 {
        let dx = self.end[0] - self.start[0];
        let dy = self.end[1] - self.start[1];
        let dz = self.end[2] - self.start[2];
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

impl TowerLayout {
    /// Derive the tower dimensions from the vehicle size. The tower stands just
    /// clear of the vehicle and spans 82% of its height, so the top platform
    /// sits below the nose while still supporting the mast.
    pub fn from_vehicle(rocket_height_m: f32, rocket_diameter_m: f32) -> Self {
        let height_m = rocket_height_m * 0.82;
        let center_x_m = rocket_diameter_m * 0.5 + 10.0;
        Self {
            height_m,
            center_x_m,
            mast_height_m: height_m + 18.0,
            column_half_span_m: COLUMN_HALF_SPAN_M,
            platform_half_span_m: PLATFORM_HALF_SPAN_M,
        }
    }

    /// Y position of each platform, from the lowest to the topmost. The topmost
    /// equals `height_m` so the mast is rooted on a platform rather than floating
    /// above the highest level.
    pub fn platform_levels_y(&self) -> [f32; TOWER_LEVEL_COUNT] {
        let mut levels = [0.0f32; TOWER_LEVEL_COUNT];
        for (index, level) in levels.iter_mut().enumerate() {
            *level = self.height_m * (index + 1) as f32 / TOWER_LEVEL_COUNT as f32;
        }
        levels
    }

    /// The four column centres on the pad-local +X/+Z plane.
    pub fn column_centers(&self) -> [[f32; 3]; 4] {
        let half = self.column_half_span_m;
        [
            [self.center_x_m - half, 0.0, -half],
            [self.center_x_m + half, 0.0, -half],
            [self.center_x_m - half, 0.0, half],
            [self.center_x_m + half, 0.0, half],
        ]
    }

    fn column_corner(&self, x_sign: f32, z_sign: f32, y: f32) -> [f32; 3] {
        [
            self.center_x_m + x_sign * self.column_half_span_m,
            y,
            z_sign * self.column_half_span_m,
        ]
    }
}

/// Horizontal perimeter rings between adjacent columns, at each bay midpoint.
/// Every endpoint coincides with a column, so the ring transfers load between
/// the columns instead of floating across the tower axis.
pub fn perimeter_braces(layout: &TowerLayout) -> Vec<TowerMember> {
    let mut members = Vec::new();
    let levels = layout.platform_levels_y();
    for bay in 0..TOWER_LEVEL_COUNT - 1 {
        let y = (levels[bay] + levels[bay + 1]) * 0.5;
        let corners = [
            layout.column_corner(-1.0, -1.0, y),
            layout.column_corner(1.0, -1.0, y),
            layout.column_corner(1.0, 1.0, y),
            layout.column_corner(-1.0, 1.0, y),
        ];
        for index in 0..4 {
            let next = (index + 1) % 4;
            members.push(TowerMember::new(
                corners[index],
                corners[next],
                MEMBER_THICKNESS_M,
            ));
        }
    }
    members
}

/// One diagonal per vertical face per bay. Each diagonal runs from a column
/// corner at the lower platform to the opposite column corner at the upper
/// platform, so both ends land on real structure.
pub fn diagonal_braces(layout: &TowerLayout) -> Vec<TowerMember> {
    let mut members = Vec::new();
    let levels = layout.platform_levels_y();
    for bay in 0..TOWER_LEVEL_COUNT - 1 {
        let lower = levels[bay];
        let upper = levels[bay + 1];
        // Alternate diagonal direction per bay so the tower reads as a braced
        // frame from every side rather than a set of parallel struts.
        let flip = bay % 2 == 1;
        for face in 0..4 {
            let (a, b) = match face {
                0 => ((-1.0, -1.0), (1.0, -1.0)),
                1 => ((1.0, -1.0), (1.0, 1.0)),
                2 => ((1.0, 1.0), (-1.0, 1.0)),
                _ => ((-1.0, 1.0), (-1.0, -1.0)),
            };
            let (start_corner, end_corner) = if flip { (b, a) } else { (a, b) };
            members.push(TowerMember::new(
                layout.column_corner(start_corner.0, start_corner.1, lower),
                layout.column_corner(end_corner.0, end_corner.1, upper),
                MEMBER_THICKNESS_M * 0.7,
            ));
        }
    }
    members
}

/// Service and umbilical arms. Each starts on the platform edge nearest the
/// vehicle and reaches the vehicle surface, so the arm is supported by the
/// platform rather than starting in the tower interior.
pub fn service_arms(layout: &TowerLayout, rocket_diameter_m: f32) -> Vec<TowerMember> {
    let levels = layout.platform_levels_y();
    let rocket_surface_x = rocket_diameter_m * 0.5;
    let start_x = layout.center_x_m - layout.platform_half_span_m;
    [3usize, 1usize]
        .into_iter()
        .map(|level| {
            let y = levels[level];
            TowerMember::new([start_x, y, 0.0], [rocket_surface_x, y, 0.0], 0.42)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> TowerLayout {
        TowerLayout::from_vehicle(60.0, 9.0)
    }

    /// Whether `point` lies within `tolerance` of any column centre on the XZ
    /// plane. Used to prove a brace end is attached to a column.
    fn touches_column(layout: &TowerLayout, point: [f32; 3], tolerance: f32) -> bool {
        layout.column_centers().iter().any(|center| {
            (point[0] - center[0]).abs() <= tolerance && (point[2] - center[2]).abs() <= tolerance
        })
    }

    fn at_platform_level(layout: &TowerLayout, y: f32, tolerance: f32) -> bool {
        layout
            .platform_levels_y()
            .iter()
            .any(|level| (y - level).abs() <= tolerance)
    }

    #[test]
    fn top_platform_supports_the_mast() {
        let layout = layout();
        let levels = layout.platform_levels_y();
        assert_eq!(levels.len(), TOWER_LEVEL_COUNT);
        assert!(
            (levels[TOWER_LEVEL_COUNT - 1] - layout.height_m).abs() < 1e-4,
            "the mast must be rooted on the top platform"
        );
        assert!(layout.mast_height_m > layout.height_m);
    }

    #[test]
    fn platforms_enclosingly_span_the_columns() {
        let layout = layout();
        assert!(
            layout.platform_half_span_m > layout.column_half_span_m,
            "platforms must extend past the columns so levels brace every leg"
        );
        for center in layout.column_centers() {
            assert!((center[0] - layout.center_x_m).abs() <= layout.platform_half_span_m + 1e-4);
        }
    }

    #[test]
    fn perimeter_braces_connect_columns() {
        let layout = layout();
        let members = perimeter_braces(&layout);
        assert_eq!(members.len(), (TOWER_LEVEL_COUNT - 1) * 4);
        for member in &members {
            assert!(
                touches_column(&layout, member.start, 1e-3),
                "brace start must meet a column: {:?}",
                member.start
            );
            assert!(
                touches_column(&layout, member.end, 1e-3),
                "brace end must meet a column: {:?}",
                member.end
            );
        }
    }

    #[test]
    fn diagonal_braces_connect_platforms_and_columns() {
        let layout = layout();
        let members = diagonal_braces(&layout);
        assert!(!members.is_empty());
        for member in &members {
            for point in [member.start, member.end] {
                assert!(
                    touches_column(&layout, point, 1e-3),
                    "diagonal must meet a column: {point:?}"
                );
                assert!(
                    at_platform_level(&layout, point[1], 1e-3),
                    "diagonal must meet a platform level: {point:?}"
                );
            }
        }
    }

    #[test]
    fn service_arms_start_on_the_platform_edge_and_reach_the_vehicle() {
        let layout = layout();
        let diameter = 9.0f32;
        let members = service_arms(&layout, diameter);
        assert_eq!(members.len(), 2);
        for member in &members {
            let platform_edge_x = layout.center_x_m - layout.platform_half_span_m;
            assert!((member.start[0] - platform_edge_x).abs() < 1e-3);
            assert!(member.start[2].abs() < 1e-3);
            assert!(
                (member.end[0] - diameter * 0.5).abs() < 1e-3,
                "arm must reach the vehicle surface"
            );
            assert!(at_platform_level(&layout, member.start[1], 1e-3));
            assert!(member.length() > 0.0);
        }
    }

    #[test]
    fn layout_is_deterministic() {
        assert_eq!(
            TowerLayout::from_vehicle(60.0, 9.0),
            TowerLayout::from_vehicle(60.0, 9.0)
        );
        assert_eq!(perimeter_braces(&layout()), perimeter_braces(&layout()));
    }
}
