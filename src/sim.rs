//! City state and simulation: grid, zones, demand feedback loop, growth.

use macroquad::rand::gen_range;

pub const GRID_W: i32 = 64;
pub const GRID_H: i32 = 64;
/// Max development level of a zone.
pub const MAX_LEVEL: u8 = 5;
/// A zone develops only if a road lies within this Chebyshev distance.
pub const ROAD_REACH: i32 = 3;

pub const ROAD_COST: f64 = 10.0;
pub const ZONE_COST: f64 = 5.0;
pub const BULLDOZE_COST: f64 = 1.0;
pub const STARTING_FUNDS: f64 = 5_000.0;

/// People housed per residential level.
const POP_PER_LEVEL: u32 = 4;
/// Jobs per commercial level.
const C_JOBS_PER_LEVEL: u32 = 3;
/// Jobs per industrial level.
const I_JOBS_PER_LEVEL: u32 = 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ZoneKind {
    Residential,
    Commercial,
    Industrial,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tile {
    Grass,
    Road,
    Zone { kind: ZoneKind, level: u8 },
}

pub struct City {
    pub tiles: Vec<Tile>,
    /// Cached per-tile road access, refreshed every tick and after edits.
    pub road_access: Vec<bool>,
    pub funds: f64,
    pub population: u32,
    pub jobs: u32,
    pub demand_r: f32,
    pub demand_c: f32,
    pub demand_i: f32,
}

impl City {
    pub fn new() -> Self {
        let n = (GRID_W * GRID_H) as usize;
        let mut city = City {
            tiles: vec![Tile::Grass; n],
            road_access: vec![false; n],
            funds: STARTING_FUNDS,
            population: 0,
            jobs: 0,
            demand_r: 0.0,
            demand_c: 0.0,
            demand_i: 0.0,
        };
        city.update_demand();
        city
    }

    pub fn idx(x: i32, y: i32) -> usize {
        (y * GRID_W + x) as usize
    }

    pub fn in_bounds(x: i32, y: i32) -> bool {
        (0..GRID_W).contains(&x) && (0..GRID_H).contains(&y)
    }

    pub fn tile(&self, x: i32, y: i32) -> Tile {
        self.tiles[Self::idx(x, y)]
    }

    /// Mark every cell within ROAD_REACH of a road.
    pub fn refresh_road_access(&mut self) {
        self.road_access.iter_mut().for_each(|b| *b = false);
        for y in 0..GRID_H {
            for x in 0..GRID_W {
                if self.tiles[Self::idx(x, y)] != Tile::Road {
                    continue;
                }
                for dy in -ROAD_REACH..=ROAD_REACH {
                    for dx in -ROAD_REACH..=ROAD_REACH {
                        let (nx, ny) = (x + dx, y + dy);
                        if Self::in_bounds(nx, ny) {
                            self.road_access[Self::idx(nx, ny)] = true;
                        }
                    }
                }
            }
        }
    }

    fn zone_levels(&self, kind: ZoneKind) -> u32 {
        self.tiles
            .iter()
            .filter_map(|t| match t {
                Tile::Zone { kind: k, level } if *k == kind => Some(*level as u32),
                _ => None,
            })
            .sum()
    }

    fn road_count(&self) -> u32 {
        self.tiles.iter().filter(|t| **t == Tile::Road).count() as u32
    }

    pub fn recompute_stats(&mut self) {
        self.population = self.zone_levels(ZoneKind::Residential) * POP_PER_LEVEL;
        self.jobs = self.zone_levels(ZoneKind::Commercial) * C_JOBS_PER_LEVEL
            + self.zone_levels(ZoneKind::Industrial) * I_JOBS_PER_LEVEL;
    }

    /// RCI feedback loop. Squashed into -1..1; small positive constants keep a
    /// young city growing, supply overshoot pushes demand negative.
    pub fn update_demand(&mut self) {
        self.recompute_stats();
        let pop = self.population as f32;
        let c_jobs = (self.zone_levels(ZoneKind::Commercial) * C_JOBS_PER_LEVEL) as f32;
        let i_jobs = (self.zone_levels(ZoneKind::Industrial) * I_JOBS_PER_LEVEL) as f32;
        let jobs = c_jobs + i_jobs;

        // People move in for jobs, leave when housing outstrips employment.
        let raw_r = (jobs - pop) + 6.0;
        // Shops want customers, saturate against their own supply.
        let raw_c = 0.35 * pop - c_jobs + 2.0;
        // Industry serves population and commerce, saturates against itself.
        let raw_i = 0.45 * pop + 0.3 * c_jobs - i_jobs + 2.0;

        fn squash(x: f32) -> f32 {
            x / (x.abs() + 12.0)
        }
        self.demand_r = squash(raw_r);
        self.demand_c = squash(raw_c);
        self.demand_i = squash(raw_i);
    }

    fn demand_for(&self, kind: ZoneKind) -> f32 {
        match kind {
            ZoneKind::Residential => self.demand_r,
            ZoneKind::Commercial => self.demand_c,
            ZoneKind::Industrial => self.demand_i,
        }
    }

    /// One fixed-interval simulation step: connectivity, demand, growth, taxes.
    pub fn tick(&mut self) {
        self.refresh_road_access();
        self.update_demand();

        for i in 0..self.tiles.len() {
            let Tile::Zone { kind, level } = self.tiles[i] else {
                continue;
            };
            let new_level = if self.road_access[i] {
                let d = self.demand_for(kind);
                if d > 0.0 && level < MAX_LEVEL && gen_range(0.0f32, 1.0) < d * 0.15 {
                    level + 1
                } else if d < -0.25 && level > 0 && gen_range(0.0f32, 1.0) < (-d - 0.25) * 0.2 {
                    level - 1
                } else {
                    level
                }
            } else if level > 0 && gen_range(0.0f32, 1.0) < 0.10 {
                // Lost road access: buildings gradually abandon.
                level - 1
            } else {
                level
            };
            if new_level != level {
                self.tiles[i] = Tile::Zone {
                    kind,
                    level: new_level,
                };
            }
        }

        // Tax income from developed zones, upkeep on roads.
        let total_levels: u32 = self.zone_levels(ZoneKind::Residential)
            + self.zone_levels(ZoneKind::Commercial)
            + self.zone_levels(ZoneKind::Industrial);
        self.funds += total_levels as f64 * 0.45 - self.road_count() as f64 * 0.05;

        self.recompute_stats();
    }

    /// Attempt a build/bulldoze at (x, y). Charges funds only when the tile
    /// actually changes. Returns Err with a reason when nothing happened.
    pub fn apply_tool(&mut self, tool: Tool, x: i32, y: i32) -> Result<(), &'static str> {
        if !Self::in_bounds(x, y) {
            return Err("out of bounds");
        }
        let i = Self::idx(x, y);
        let current = self.tiles[i];
        let (new_tile, cost) = match tool {
            Tool::Road => {
                if current != Tile::Grass {
                    return Err("roads need empty ground");
                }
                (Tile::Road, ROAD_COST)
            }
            Tool::Bulldoze => {
                if current == Tile::Grass {
                    return Err("nothing to bulldoze");
                }
                (Tile::Grass, BULLDOZE_COST)
            }
            Tool::Zone(kind) => {
                if current != Tile::Grass {
                    return Err("zones need empty ground");
                }
                (Tile::Zone { kind, level: 0 }, ZONE_COST)
            }
        };
        if self.funds < cost {
            return Err("not enough funds");
        }
        self.funds -= cost;
        self.tiles[i] = new_tile;
        self.refresh_road_access();
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Road,
    Bulldoze,
    Zone(ZoneKind),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a city with a horizontal road and a strip of zones beside it.
    fn city_with_road_and_zones(kind: ZoneKind) -> City {
        let mut city = City::new();
        for x in 0..20 {
            city.apply_tool(Tool::Road, x, 10).unwrap();
        }
        for x in 0..20 {
            city.apply_tool(Tool::Zone(kind), x, 11).unwrap();
        }
        city
    }

    fn total_level(city: &City, kind: ZoneKind) -> u32 {
        city.tiles
            .iter()
            .filter_map(|t| match t {
                Tile::Zone { kind: k, level } if *k == kind => Some(*level as u32),
                _ => None,
            })
            .sum()
    }

    #[test]
    fn connected_residential_develops_unconnected_stays_dirt() {
        let mut city = city_with_road_and_zones(ZoneKind::Residential);
        // A far-away zone with no road anywhere near it.
        city.apply_tool(Tool::Zone(ZoneKind::Residential), 50, 50).unwrap();
        for _ in 0..60 {
            city.tick();
        }
        assert!(
            total_level(&city, ZoneKind::Residential) > 0,
            "road-adjacent residential should develop under positive demand"
        );
        assert_eq!(
            city.tile(50, 50),
            Tile::Zone { kind: ZoneKind::Residential, level: 0 },
            "zone without road access must stay undeveloped"
        );
        assert!(city.population > 0);
    }

    #[test]
    fn residential_demand_drops_when_housing_far_exceeds_jobs() {
        let mut city = city_with_road_and_zones(ZoneKind::Residential);
        let initial = city.demand_r;
        assert!(initial > 0.0, "young city should want residents");
        for _ in 0..200 {
            city.tick();
        }
        assert!(
            city.demand_r < initial,
            "housing with no jobs should push R demand down (was {initial}, now {})",
            city.demand_r
        );
        assert!(city.demand_c > 0.0, "population should create commercial demand");
        assert!(city.demand_i > 0.0, "population should create industrial demand");
    }

    #[test]
    fn bulldozing_road_causes_abandonment() {
        let mut city = city_with_road_and_zones(ZoneKind::Residential);
        for _ in 0..60 {
            city.tick();
        }
        assert!(total_level(&city, ZoneKind::Residential) > 0);
        for x in 0..20 {
            city.apply_tool(Tool::Bulldoze, x, 10).unwrap();
        }
        for _ in 0..300 {
            city.tick();
        }
        assert_eq!(
            total_level(&city, ZoneKind::Residential),
            0,
            "zones that lost road access should decay back to dirt"
        );
    }

    #[test]
    fn mixed_city_grows_and_earns_net_tax_income() {
        // Road row with residential on one side, commercial + industrial on
        // the other: the full feedback loop (jobs <-> population).
        let mut city = City::new();
        for x in 0..20 {
            city.apply_tool(Tool::Road, x, 10).unwrap();
            city.apply_tool(Tool::Zone(ZoneKind::Residential), x, 11).unwrap();
            let kind = if x < 10 { ZoneKind::Commercial } else { ZoneKind::Industrial };
            city.apply_tool(Tool::Zone(kind), x, 9).unwrap();
        }
        for _ in 0..100 {
            city.tick();
        }
        assert!(city.population > 50, "mixed city should grow (pop {})", city.population);
        assert!(city.jobs > 50, "mixed city should create jobs (jobs {})", city.jobs);
        let funds_before = city.funds;
        for _ in 0..50 {
            city.tick();
        }
        assert!(
            city.funds > funds_before,
            "developed city should earn net tax income ({funds_before} -> {})",
            city.funds
        );
    }

    #[test]
    fn build_costs_charged_and_invalid_builds_rejected() {
        let mut city = City::new();
        assert_eq!(city.funds, STARTING_FUNDS);
        city.apply_tool(Tool::Road, 5, 5).unwrap();
        assert_eq!(city.funds, STARTING_FUNDS - ROAD_COST);
        assert!(city.apply_tool(Tool::Road, 5, 5).is_err(), "no double-build");
        assert!(
            city.apply_tool(Tool::Zone(ZoneKind::Commercial), 5, 5).is_err(),
            "zones only on grass"
        );
        city.apply_tool(Tool::Bulldoze, 5, 5).unwrap();
        assert_eq!(city.tile(5, 5), Tile::Grass);
        assert!(city.apply_tool(Tool::Bulldoze, 5, 5).is_err());
        assert!(city.apply_tool(Tool::Road, -1, 3).is_err());

        city.funds = 1.0;
        assert_eq!(city.apply_tool(Tool::Road, 6, 6), Err("not enough funds"));
        assert_eq!(city.tile(6, 6), Tile::Grass);
    }
}
