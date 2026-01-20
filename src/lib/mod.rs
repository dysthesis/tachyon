use mlua::{Lua, Result, Table, UserData, UserDataMethods};
use nucleo::{
    Config, Matcher, Utf32String,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};

struct Tachyon {
    /// The list of items to match against
    items: Vec<Utf32String>,
    /// The fuzzy matcher engine, containing the memory to be used and reused
    /// throughout the matching process.
    matcher: Matcher, // reused to prevent repeated heap allocs on the hot path
    /// The pattern to look for in `items`.
    pattern: Pattern,
    /// How to treat case mismatch between two characters
    case_matching: CaseMatching,
    /// Unicode handling normalissation
    normalisation: Normalization,
    literal: bool,
    indices_buf: Vec<u32>,
}

impl Tachyon {
    fn new(items: Vec<String>, match_paths: bool, literal: bool) -> Self {
        let cfg = if match_paths {
            Config::DEFAULT.match_paths()
        } else {
            Config::DEFAULT
        };

        Self {
            items: items.into_iter().map(Utf32String::from).collect(),
            matcher: Matcher::new(cfg),
            pattern: Pattern::default(),
            case_matching: CaseMatching::Smart,
            normalisation: Normalization::Smart,
            literal,
            indices_buf: Vec::new(),
        }
    }

    fn set_query(&mut self, query: &str) {
        // "Literal" fuzzy matching means that there are no special parsing for
        // characters such as ^ ' | $.
        if self.literal {
            self.pattern = Pattern::new(
                query,
                self.case_matching,
                self.normalisation,
                AtomKind::Fuzzy,
            )
        }
        // "Parsed" fuzzy matching treats special characters such as ^ ' | $
        // with special meaning at word boundaries (like fzf).
        else {
            self.pattern
                .reparse(query, self.case_matching, self.normalisation);
        }
    }

    fn match_idx(&mut self, idxs: Vec<usize>, query: String, limit: Option<usize>) -> Vec<usize> {
        if query.is_empty() {
            return idxs;
        }
        self.set_query(&query);

        let mut scored: Vec<(usize, u32)> = Vec::with_capacity(idxs.len());

        idxs.iter()
            .copied()
            // Convert back to base 0 since Lua uses base 1
            .filter_map(|lua_idx| lua_idx.checked_sub(1).map(|idx| (lua_idx, idx)))
            // We don't have items with that index
            .filter(|(_, idx)| *idx < self.items.len())
            // Get the associated item
            .map(|(lua_idx, idx)| (lua_idx, self.items[idx].slice(..)))
            // Score it with the fuzzy matcher against the pattern
            .filter_map(|(lua_idx, hay)| {
                self.pattern
                    .score(hay, &mut self.matcher)
                    .map(|score| (lua_idx, score))
            })
            .for_each(|pair| scored.push(pair));

        scored.sort_by(|(left_idx, left_score), (right_idx, right_score)| {
            right_score
                .cmp(left_score)
                .then_with(|| left_idx.cmp(right_idx))
        });

        let mut out: Vec<usize> = scored.into_iter().map(|(i, _)| i).collect();
        if let Some(lim) = limit {
            out.truncate(lim);
        }
        out
    }

    fn match_indices(&mut self, lua_idx: usize, query: String) -> Option<Vec<u32>> {
        let idx = lua_idx.saturating_sub(1);
        if idx >= self.items.len() {
            return None;
        }
        if query.is_empty() {
            return Some(Vec::new());
        }

        self.set_query(&query);

        self.indices_buf.clear();
        let hay = self.items[idx].slice(..);
        let _score = self
            .pattern
            .indices(hay, &mut self.matcher, &mut self.indices_buf)?;

        // Pattern::indices appends per-atom indices, so we deduplicate for
        // highlighting.
        self.indices_buf.sort_unstable();
        self.indices_buf.dedup();

        Some(self.indices_buf.clone())
    }
}

impl UserData for Tachyon {
    fn add_methods<M>(methods: &mut M)
    where
        M: UserDataMethods<Self>,
    {
        methods.add_method_mut(
            "match",
            |_, this, (inds, query, limit): (Vec<usize>, String, Option<usize>)| {
                Ok(this.match_idx(inds, query, limit))
            },
        );

        methods.add_method_mut("indices", |_, this, (index, query): (usize, String)| {
            Ok(this.match_indices(index, query))
        });
    }
}

#[mlua::lua_module]
fn tachyon(lua: &Lua) -> Result<Table> {
    let exports = lua.create_table()?;

    exports.set(
        "new",
        lua.create_function(|lua, (items, opts): (Vec<String>, Option<Table>)| {
            lua.globals()
                .get::<mlua::Function>("print")?
                .call::<()>("Called the `new` function in tachyon!")?;
            let opts = opts;
            let match_paths = match &opts {
                Some(t) => t.get::<bool>("match_paths").unwrap_or(true),
                None => true,
            };
            let literal = match &opts {
                Some(t) => t.get::<bool>("literal").unwrap_or(false),
                None => false,
            };

            lua.create_userdata(Tachyon::new(items, match_paths, literal))
        })?,
    )?;

    exports.set(
        "match",
        lua.create_function(
            |lua,
             (stritems, inds, query, _opts): (Table, Table, Table, Option<Table>)|
             -> Result<Table> {
                // Reconstruct the prompt from MiniPick-style query table.
                let mut needle = String::new();
                for part in query.sequence_values::<mlua::String>() {
                    let part = part?;
                    needle.push_str(&part.to_str()?);
                }

                let out = lua.create_table()?;
                let mut out_i: i64 = 1;

                // If query is empty, return `inds` unchanged.
                if needle.is_empty() {
                    for idx in inds.sequence_values::<i64>() {
                        out.set(out_i, idx?)?;
                        out_i += 1;
                    }
                    return Ok(out);
                }

                // Filter by substring containment.
                for idx in inds.sequence_values::<i64>() {
                    let idx = idx?;
                    let hay: mlua::String = stritems.get(idx)?;
                    if hay.to_str()?.contains(&needle) {
                        out.set(out_i, idx)?;
                        out_i += 1;
                    }
                }

                Ok(out)
            },
        )?,
    )?;
    Ok(exports)
}
