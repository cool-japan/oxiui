//! Pure-Rust Cassowary linear constraint solver.
//!
//! Implements the dual-phase simplex restricted tableau (Badros & Borning 1997),
//! faithful to the cassowary-rs / kiwi reference structure. No external crates
//! are used.
//!
//! ## Quick start
//!
//! ```rust
//! use oxiui_core::solver::{Solver, Variable, Constraint, Expression, Term, RelOp, Strength};
//!
//! let mut solver = Solver::new();
//! let x = Variable::new();
//! solver.add_constraint(Constraint::new(
//!     Expression::new(vec![Term { variable: x.clone(), coefficient: 1.0 }], -42.0),
//!     RelOp::Equal,
//!     Strength::REQUIRED,
//! )).unwrap();
//! solver.update_variables();
//! assert!((solver.value_of(&x) - 42.0).abs() < 1e-6);
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

// ── Atomic id generators ────────────────────────────────────────────────────

static NEXT_VAR_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_SYM_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_CONSTRAINT_ID: AtomicU64 = AtomicU64::new(1);

/// Coefficients below this magnitude are treated as zero.
const NEAR_ZERO: f64 = 1.0e-8;

fn near_zero(v: f64) -> bool {
    v.abs() < NEAR_ZERO
}

// ── Private tableau types ───────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum SymKind {
    Invalid,
    External,
    Slack,
    Error,
    Dummy,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Sym(u64, SymKind);

impl Sym {
    fn invalid() -> Self {
        Sym(0, SymKind::Invalid)
    }

    fn is_invalid(self) -> bool {
        self.1 == SymKind::Invalid
    }

    fn is_external(self) -> bool {
        self.1 == SymKind::External
    }

    fn is_error(self) -> bool {
        self.1 == SymKind::Error
    }

    fn is_dummy(self) -> bool {
        self.1 == SymKind::Dummy
    }

    fn is_pivotable(self) -> bool {
        matches!(self.1, SymKind::Slack | SymKind::Error)
    }
}

fn new_sym(kind: SymKind) -> Sym {
    Sym(NEXT_SYM_ID.fetch_add(1, Ordering::Relaxed), kind)
}

// ── Tableau Row ─────────────────────────────────────────────────────────────

/// A linear row: `key_symbol = constant + Σ cells[s] * s`.
#[derive(Clone, Debug, Default)]
struct Row {
    cells: HashMap<Sym, f64>,
    constant: f64,
}

impl Row {
    fn new(constant: f64) -> Self {
        Row {
            cells: HashMap::new(),
            constant,
        }
    }

    /// Add `v` to the constant; return the new constant.
    fn add_constant(&mut self, v: f64) -> f64 {
        self.constant += v;
        self.constant
    }

    fn insert_symbol(&mut self, s: Sym, coeff: f64) {
        let entry = self.cells.entry(s).or_insert(0.0);
        *entry += coeff;
        if near_zero(*entry) {
            self.cells.remove(&s);
        }
    }

    fn insert_row(&mut self, other: &Row, coeff: f64) {
        self.constant += other.constant * coeff;
        for (&s, &c) in &other.cells {
            let entry = self.cells.entry(s).or_insert(0.0);
            *entry += c * coeff;
            if near_zero(*entry) {
                self.cells.remove(&s);
            }
        }
    }

    fn remove(&mut self, s: Sym) {
        self.cells.remove(&s);
    }

    fn reverse_sign(&mut self) {
        self.constant = -self.constant;
        for v in self.cells.values_mut() {
            *v = -*v;
        }
    }

    /// Isolate `s` on the left-hand side: divide by `-coeff[s]`, remove `s`.
    fn solve_for_symbol(&mut self, s: Sym) {
        let c = self.cells.remove(&s).unwrap_or(1.0);
        let factor = -1.0 / c;
        self.constant *= factor;
        for v in self.cells.values_mut() {
            *v *= factor;
        }
    }

    /// Pivot `lhs` (leaving) and `rhs` (entering).
    ///
    /// Precondition: `lhs` is NOT in `self.cells` (it is the implicit key).
    /// Postcondition: `self` can be stored as `rows[rhs]`.
    ///
    /// Algorithm (matches cassowary-rs verbatim):
    ///   1. Insert `lhs` with coefficient -1.
    ///   2. Call `solve_for_symbol(rhs)`.
    fn solve_for_symbols(&mut self, lhs: Sym, rhs: Sym) {
        self.insert_symbol(lhs, -1.0);
        self.solve_for_symbol(rhs);
    }

    fn coefficient_for(&self, s: Sym) -> f64 {
        *self.cells.get(&s).unwrap_or(&0.0)
    }

    fn substitute(&mut self, s: Sym, row: &Row) {
        if let Some(coeff) = self.cells.remove(&s) {
            self.insert_row(row, coeff);
        }
    }
}

// ── Public API types ────────────────────────────────────────────────────────

/// A layout variable whose value is determined by the constraint solver.
#[derive(Clone, Debug)]
pub struct Variable {
    id: u64,
}

impl Variable {
    /// Create a new unique variable.
    pub fn new() -> Self {
        Variable {
            id: NEXT_VAR_ID.fetch_add(1, Ordering::Relaxed),
        }
    }
}

impl Default for Variable {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for Variable {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for Variable {}
impl std::hash::Hash for Variable {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// A variable multiplied by a coefficient.
#[derive(Clone, Debug)]
pub struct Term {
    /// The variable.
    pub variable: Variable,
    /// The coefficient applied to the variable.
    pub coefficient: f64,
}

/// A linear expression: `Σ coefficient·variable + constant`.
#[derive(Clone, Debug, Default)]
pub struct Expression {
    /// Weighted variable terms.
    pub terms: Vec<Term>,
    /// Constant offset.
    pub constant: f64,
}

impl Expression {
    /// Construct from an explicit list of terms and a constant.
    pub fn new(terms: Vec<Term>, constant: f64) -> Self {
        Expression { terms, constant }
    }

    /// A constant expression with no variables.
    pub fn from_constant(c: f64) -> Self {
        Expression {
            terms: Vec::new(),
            constant: c,
        }
    }

    /// An expression representing a single variable with coefficient `1.0`.
    pub fn from_variable(v: Variable) -> Self {
        Expression {
            terms: vec![Term {
                variable: v,
                coefficient: 1.0,
            }],
            constant: 0.0,
        }
    }
}

/// Relational operator for a constraint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RelOp {
    /// `lhs ≤ rhs`.
    LessThanOrEq,
    /// `lhs ≥ rhs`.
    GreaterThanOrEq,
    /// `lhs = rhs`.
    Equal,
}

/// Predefined constraint strengths.
pub struct Strength;

impl Strength {
    /// Required constraint — must be satisfied or the solver returns an error.
    pub const REQUIRED: f64 = 1_001_001_000.0;
    /// Strong soft constraint.
    pub const STRONG: f64 = 1_000_000.0;
    /// Medium soft constraint.
    pub const MEDIUM: f64 = 1_000.0;
    /// Weak soft constraint.
    pub const WEAK: f64 = 1.0;

    /// Clamp `s` to be at most [`Strength::REQUIRED`].
    pub fn clip(s: f64) -> f64 {
        s.min(Self::REQUIRED)
    }
}

/// A linear constraint with a relational operator and a priority strength.
#[derive(Clone, Debug)]
pub struct Constraint {
    expression: Expression,
    op: RelOp,
    strength: f64,
    id: u64,
}

impl Constraint {
    /// Create a new constraint.
    pub fn new(expression: Expression, op: RelOp, strength: f64) -> Self {
        let id = NEXT_CONSTRAINT_ID.fetch_add(1, Ordering::Relaxed);
        Constraint {
            expression,
            op,
            strength: Strength::clip(strength),
            id,
        }
    }
}

impl PartialEq for Constraint {
    fn eq(&self, o: &Self) -> bool {
        self.id == o.id
    }
}
impl Eq for Constraint {}
impl std::hash::Hash for Constraint {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// Errors produced by the solver.
#[derive(Clone, Debug, PartialEq)]
pub enum SolverError {
    /// The constraint was already added.
    DuplicateConstraint,
    /// The constraint set is over-constrained (REQUIRED constraints conflict).
    UnsatisfiableConstraint,
    /// The constraint was not previously added.
    UnknownConstraint,
    /// The variable was not registered as an edit variable.
    UnknownEditVariable,
    /// An internal solver invariant was violated.
    InternalError(String),
}

impl std::fmt::Display for SolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SolverError::DuplicateConstraint => write!(f, "duplicate constraint"),
            SolverError::UnsatisfiableConstraint => {
                write!(f, "unsatisfiable constraint (over-constrained)")
            }
            SolverError::UnknownConstraint => write!(f, "unknown constraint"),
            SolverError::UnknownEditVariable => write!(f, "unknown edit variable"),
            SolverError::InternalError(s) => write!(f, "internal solver error: {s}"),
        }
    }
}

impl std::error::Error for SolverError {}

// ── Internal bookkeeping ────────────────────────────────────────────────────

struct Tag {
    marker: Sym,
    other: Sym,
}

struct EditInfo {
    tag: Tag,
    constant: f64,
}

// ── Solver ──────────────────────────────────────────────────────────────────

/// The Cassowary incremental constraint solver.
///
/// Call [`update_variables`](Solver::update_variables) after all constraints
/// and suggestions have been applied, then read values with
/// [`value_of`](Solver::value_of).
pub struct Solver {
    rows: HashMap<Sym, Row>,
    /// Variable id → its External symbol.
    vars: HashMap<u64, Sym>,
    /// Constraint id → tag.
    constraints: HashMap<u64, Tag>,
    /// Edit variable id → bookkeeping.
    edits: HashMap<u64, EditInfo>,
    /// Restricted basic variables with a negative constant.
    infeasible_rows: Vec<Sym>,
    /// Objective row: minimise weighted sum of error symbols.
    objective: Row,
    /// Cached variable values (populated by `update_variables`).
    values: HashMap<u64, f64>,
}

impl Solver {
    /// Create a new empty solver.
    pub fn new() -> Self {
        Solver {
            rows: HashMap::new(),
            vars: HashMap::new(),
            constraints: HashMap::new(),
            edits: HashMap::new(),
            infeasible_rows: Vec::new(),
            objective: Row::new(0.0),
            values: HashMap::new(),
        }
    }

    // ── Public API ────────────────────────────────────────────────────────

    /// Add a constraint to the solver.
    pub fn add_constraint(&mut self, constraint: Constraint) -> Result<(), SolverError> {
        if self.constraints.contains_key(&constraint.id) {
            return Err(SolverError::DuplicateConstraint);
        }

        let (mut row, tag) = self.create_row(&constraint);
        let subject = Self::choose_subject(&row, &tag);

        if subject.is_invalid() {
            // All-dummy row: linearly dependent REQUIRED constraint.
            if Self::all_dummies(&row) && !near_zero(row.constant) {
                return Err(SolverError::UnsatisfiableConstraint);
            }
            // Use an artificial variable for phase-1 feasibility.
            if !self.add_with_artificial(&row)? {
                return Err(SolverError::UnsatisfiableConstraint);
            }
        } else {
            row.solve_for_symbol(subject);
            self.substitute_all(subject, &row);
            self.rows.insert(subject, row);
        }

        self.constraints.insert(constraint.id, tag);
        self.optimize()?;
        Ok(())
    }

    /// Remove a previously added constraint.
    pub fn remove_constraint(&mut self, constraint: &Constraint) -> Result<(), SolverError> {
        let tag = self
            .constraints
            .remove(&constraint.id)
            .ok_or(SolverError::UnknownConstraint)?;

        self.remove_constraint_effects(&tag, constraint.strength);

        // If marker is basic, just remove that row.
        if self.rows.remove(&tag.marker).is_none() {
            // Marker is non-basic: pivot it into the basis, then remove.
            let (leaving, mut row) = self
                .get_marker_leaving_row(tag.marker)
                .ok_or_else(|| SolverError::InternalError("no leaving row for marker".into()))?;
            row.solve_for_symbols(leaving, tag.marker);
            self.substitute_all(tag.marker, &row);
            // Now marker IS basic — remove it.
            self.rows.remove(&tag.marker);
        }

        self.optimize()?;
        Ok(())
    }

    /// Register `variable` as an edit variable at the given `strength`.
    ///
    /// Must be called before [`Solver::suggest_value`]. Strength must not be
    /// [`Strength::REQUIRED`].
    pub fn add_edit_variable(
        &mut self,
        variable: &Variable,
        strength: f64,
    ) -> Result<(), SolverError> {
        if self.edits.contains_key(&variable.id) {
            return Err(SolverError::DuplicateConstraint);
        }
        let strength = Strength::clip(strength);
        if (strength - Strength::REQUIRED).abs() < NEAR_ZERO {
            return Err(SolverError::InternalError(
                "edit variables cannot be REQUIRED".into(),
            ));
        }

        let expr = Expression::from_variable(variable.clone());
        let c = Constraint::new(expr, RelOp::Equal, strength);
        self.add_constraint(c.clone())?;

        let tag = self
            .constraints
            .get(&c.id)
            .ok_or_else(|| SolverError::InternalError("tag not found after add".into()))?;
        let tag = Tag {
            marker: tag.marker,
            other: tag.other,
        };
        self.edits
            .insert(variable.id, EditInfo { tag, constant: 0.0 });
        Ok(())
    }

    /// Suggest a value for a registered edit variable.
    pub fn suggest_value(&mut self, variable: &Variable, value: f64) -> Result<(), SolverError> {
        let (delta, marker, other) = {
            let info = self
                .edits
                .get_mut(&variable.id)
                .ok_or(SolverError::UnknownEditVariable)?;
            let delta = value - info.constant;
            info.constant = value;
            (delta, info.tag.marker, info.tag.other)
        };

        // Mirror cassowary-rs's three-case suggest logic.
        // Case 1: marker is basic.
        if let Some(row) = self.rows.get_mut(&marker) {
            if row.add_constant(-delta) < 0.0 {
                self.infeasible_rows.push(marker);
            }
            return self.dual_optimize();
        }
        // Case 2: other is basic.
        if !other.is_invalid() {
            if let Some(row) = self.rows.get_mut(&other) {
                if row.add_constant(delta) < 0.0 {
                    self.infeasible_rows.push(other);
                }
                return self.dual_optimize();
            }
        }
        // Case 3: neither basic — adjust all rows by coefficient.
        let keys: Vec<Sym> = self.rows.keys().cloned().collect();
        for sym in keys {
            let row = self.rows.get_mut(&sym).expect("row present");
            let coeff = row.coefficient_for(marker);
            let diff = delta * coeff;
            if !near_zero(diff) {
                row.add_constant(diff);
                if !sym.is_external() && row.constant < 0.0 {
                    self.infeasible_rows.push(sym);
                }
            }
        }
        self.dual_optimize()
    }

    /// Populate cached variable values.
    ///
    /// Call after all constraints and suggestions have been applied, then
    /// read values with [`Solver::value_of`].
    pub fn update_variables(&mut self) {
        let pairs: Vec<(u64, Sym)> = self.vars.iter().map(|(&id, &s)| (id, s)).collect();
        for (var_id, sym) in pairs {
            let v = self.rows.get(&sym).map(|r| r.constant).unwrap_or(0.0);
            self.values.insert(var_id, v);
        }
    }

    /// Return the last computed value for `variable` (0.0 if not yet solved).
    pub fn value_of(&self, variable: &Variable) -> f64 {
        *self.values.get(&variable.id).unwrap_or(&0.0)
    }

    /// Reset the solver to the empty starting state.
    pub fn reset(&mut self) {
        self.rows.clear();
        self.vars.clear();
        self.constraints.clear();
        self.edits.clear();
        self.infeasible_rows.clear();
        self.objective = Row::new(0.0);
        self.values.clear();
    }

    // ── Internal helpers ──────────────────────────────────────────────────

    fn get_var_sym(&mut self, variable: &Variable) -> Sym {
        *self
            .vars
            .entry(variable.id)
            .or_insert_with(|| new_sym(SymKind::External))
    }

    fn create_row(&mut self, constraint: &Constraint) -> (Row, Tag) {
        let expr = &constraint.expression;
        let mut row = Row::new(expr.constant);
        for term in &expr.terms {
            if !near_zero(term.coefficient) {
                let sym = self.get_var_sym(&term.variable);
                if let Some(basic) = self.rows.get(&sym).cloned() {
                    row.insert_row(&basic, term.coefficient);
                } else {
                    row.insert_symbol(sym, term.coefficient);
                }
            }
        }

        let is_req = (constraint.strength - Strength::REQUIRED).abs() < NEAR_ZERO;

        let tag = match constraint.op {
            RelOp::GreaterThanOrEq | RelOp::LessThanOrEq => {
                let coeff = if constraint.op == RelOp::LessThanOrEq {
                    1.0
                } else {
                    -1.0
                };
                let slack = new_sym(SymKind::Slack);
                row.insert_symbol(slack, coeff);
                if !is_req {
                    let error = new_sym(SymKind::Error);
                    row.insert_symbol(error, -coeff);
                    self.objective.insert_symbol(error, constraint.strength);
                    Tag {
                        marker: slack,
                        other: error,
                    }
                } else {
                    Tag {
                        marker: slack,
                        other: Sym::invalid(),
                    }
                }
            }
            RelOp::Equal => {
                if is_req {
                    let dummy = new_sym(SymKind::Dummy);
                    row.insert_symbol(dummy, 1.0);
                    Tag {
                        marker: dummy,
                        other: Sym::invalid(),
                    }
                } else {
                    let ep = new_sym(SymKind::Error);
                    let em = new_sym(SymKind::Error);
                    row.insert_symbol(ep, -1.0);
                    row.insert_symbol(em, 1.0);
                    self.objective.insert_symbol(ep, constraint.strength);
                    self.objective.insert_symbol(em, constraint.strength);
                    Tag {
                        marker: ep,
                        other: em,
                    }
                }
            }
        };

        if row.constant < 0.0 {
            row.reverse_sign();
        }
        (row, tag)
    }

    fn choose_subject(row: &Row, tag: &Tag) -> Sym {
        for &sym in row.cells.keys() {
            if sym.is_external() {
                return sym;
            }
        }
        if tag.marker.is_pivotable() && row.coefficient_for(tag.marker) < 0.0 {
            return tag.marker;
        }
        if !tag.other.is_invalid()
            && tag.other.is_pivotable()
            && row.coefficient_for(tag.other) < 0.0
        {
            return tag.other;
        }
        Sym::invalid()
    }

    fn all_dummies(row: &Row) -> bool {
        row.cells.keys().all(|s| s.is_dummy())
    }

    fn add_with_artificial(&mut self, row: &Row) -> Result<bool, SolverError> {
        let art = new_sym(SymKind::Slack);
        // The artificial objective row is a copy of the constraint row.
        let mut art_obj = row.clone();
        self.rows.insert(art, row.clone());

        // Optimise the artificial objective.
        self.optimize_row(&mut art_obj)?;

        let success = near_zero(art_obj.constant);

        // If art is still in the basis, pivot it out.
        if let Some(mut art_row) = self.rows.remove(&art) {
            if !art_row.cells.is_empty() {
                let entering = Self::any_pivotable_sym(&art_row);
                if !entering.is_invalid() {
                    art_row.solve_for_symbols(art, entering);
                    self.substitute_all(entering, &art_row);
                    self.rows.insert(entering, art_row);
                }
            }
        }

        // Remove art from all remaining rows and objective.
        for row in self.rows.values_mut() {
            row.remove(art);
        }
        self.objective.remove(art);

        Ok(success)
    }

    fn any_pivotable_sym(row: &Row) -> Sym {
        row.cells
            .keys()
            .find(|&&s| s.is_pivotable())
            .cloned()
            .unwrap_or_else(Sym::invalid)
    }

    fn substitute_all(&mut self, sym: Sym, row: &Row) {
        let keys: Vec<Sym> = self.rows.keys().cloned().collect();
        for key in keys {
            let r = self.rows.get_mut(&key).expect("row present");
            r.substitute(sym, row);
            if !key.is_external() && r.constant < 0.0 {
                self.infeasible_rows.push(key);
            }
        }
        self.objective.substitute(sym, row);
    }

    fn optimize(&mut self) -> Result<(), SolverError> {
        loop {
            let entering = Self::get_entering_sym(&self.objective);
            if entering.is_invalid() {
                return Ok(());
            }
            let (leaving, mut row) = self
                .get_leaving_row(entering)
                .ok_or_else(|| SolverError::InternalError("objective unbounded".into()))?;
            row.solve_for_symbols(leaving, entering);
            self.substitute_all(entering, &row);
            self.rows.insert(entering, row);
        }
    }

    fn optimize_row(&mut self, obj: &mut Row) -> Result<(), SolverError> {
        loop {
            let entering = Self::get_entering_sym(obj);
            if entering.is_invalid() {
                return Ok(());
            }
            let (leaving, mut row) = self
                .get_leaving_row(entering)
                .ok_or_else(|| SolverError::InternalError("art objective unbounded".into()))?;
            row.solve_for_symbols(leaving, entering);
            self.substitute_all(entering, &row);
            obj.substitute(entering, &row);
            self.rows.insert(entering, row);
        }
    }

    fn dual_optimize(&mut self) -> Result<(), SolverError> {
        while let Some(leaving) = self.infeasible_rows.pop() {
            // Only process if the row is still infeasible.
            let constant = match self.rows.get(&leaving) {
                Some(r) => r.constant,
                None => continue,
            };
            if constant >= 0.0 {
                continue;
            }

            // Get entering: symbol with positive coeff in the infeasible row
            // and minimum ratio of obj_coeff / row_coeff.
            let entering = {
                let row = self.rows.get(&leaving).expect("infeasible row");
                let mut best = Sym::invalid();
                let mut ratio = f64::INFINITY;
                for (&sym, &coeff) in &row.cells {
                    if coeff > 0.0 && !sym.is_dummy() {
                        let obj_c = self.objective.coefficient_for(sym);
                        let r = obj_c / coeff;
                        if r < ratio {
                            ratio = r;
                            best = sym;
                        }
                    }
                }
                best
            };

            if entering.is_invalid() {
                return Err(SolverError::InternalError("dual optimize failed".into()));
            }

            let mut row = self
                .rows
                .remove(&leaving)
                .ok_or_else(|| SolverError::InternalError("leaving row missing".into()))?;
            row.solve_for_symbols(leaving, entering);
            self.substitute_all(entering, &row);
            self.rows.insert(entering, row);
        }
        Ok(())
    }

    fn get_entering_sym(obj: &Row) -> Sym {
        for (&sym, &coeff) in &obj.cells {
            if !sym.is_dummy() && coeff < 0.0 {
                return sym;
            }
        }
        Sym::invalid()
    }

    fn get_leaving_row(&mut self, entering: Sym) -> Option<(Sym, Row)> {
        let mut ratio = f64::INFINITY;
        let mut found = None;
        for (&sym, row) in &self.rows {
            if sym.is_external() {
                continue;
            }
            let c = row.coefficient_for(entering);
            if c < 0.0 {
                let r = -row.constant / c;
                if r < ratio {
                    ratio = r;
                    found = Some(sym);
                }
            }
        }
        found.map(|s| (s, self.rows.remove(&s).expect("row present")))
    }

    fn get_marker_leaving_row(&mut self, marker: Sym) -> Option<(Sym, Row)> {
        let mut r1 = f64::INFINITY;
        let mut r2 = f64::INFINITY;
        let mut first: Option<Sym> = None;
        let mut second: Option<Sym> = None;
        let mut third: Option<Sym> = None;

        for (&sym, row) in &self.rows {
            let c = row.coefficient_for(marker);
            if near_zero(c) {
                continue;
            }
            if sym.is_external() {
                third = Some(sym);
            } else if c < 0.0 {
                let r = -row.constant / c;
                if r < r1 {
                    r1 = r;
                    first = Some(sym);
                }
            } else {
                let r = row.constant / c;
                if r < r2 {
                    r2 = r;
                    second = Some(sym);
                }
            }
        }

        first
            .or(second)
            .or(third)
            .map(|s| (s, self.rows.remove(&s).expect("row present")))
    }

    fn remove_constraint_effects(&mut self, tag: &Tag, strength: f64) {
        if tag.marker.is_error() {
            self.remove_marker_effects(tag.marker, strength);
        } else if tag.other.is_error() {
            self.remove_marker_effects(tag.other, strength);
        }
    }

    fn remove_marker_effects(&mut self, marker: Sym, strength: f64) {
        if let Some(row) = self.rows.get(&marker).cloned() {
            self.objective.insert_row(&row, -strength);
        } else {
            self.objective.insert_symbol(marker, -strength);
        }
    }
}

impl Default for Solver {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-4
    }

    /// 1. Two REQUIRED equality constraints: x == 10, y == x + 5  →  x=10, y=15.
    #[test]
    fn two_equality_constraints() {
        let mut s = Solver::new();
        let x = Variable::new();
        let y = Variable::new();

        // x - 10 == 0
        s.add_constraint(Constraint::new(
            Expression::new(
                vec![Term {
                    variable: x.clone(),
                    coefficient: 1.0,
                }],
                -10.0,
            ),
            RelOp::Equal,
            Strength::REQUIRED,
        ))
        .unwrap();
        // y - x - 5 == 0
        s.add_constraint(Constraint::new(
            Expression::new(
                vec![
                    Term {
                        variable: y.clone(),
                        coefficient: 1.0,
                    },
                    Term {
                        variable: x.clone(),
                        coefficient: -1.0,
                    },
                ],
                -5.0,
            ),
            RelOp::Equal,
            Strength::REQUIRED,
        ))
        .unwrap();

        s.update_variables();
        assert!(approx(s.value_of(&x), 10.0), "x={}", s.value_of(&x));
        assert!(approx(s.value_of(&y), 15.0), "y={}", s.value_of(&y));
    }

    /// 2. suggest_value and update_variables: edit x toward 30.
    #[test]
    fn suggest_value_and_update() {
        let mut s = Solver::new();
        let x = Variable::new();

        // Weak stay: x == 0.
        s.add_constraint(Constraint::new(
            Expression::from_variable(x.clone()),
            RelOp::Equal,
            Strength::WEAK,
        ))
        .unwrap();

        s.add_edit_variable(&x, Strength::STRONG).unwrap();
        s.suggest_value(&x, 30.0).unwrap();
        s.update_variables();

        assert!(approx(s.value_of(&x), 30.0), "x={}", s.value_of(&x));
    }

    /// 3. Over-constrained REQUIRED → UnsatisfiableConstraint.
    #[test]
    fn over_constrained_required() {
        let mut s = Solver::new();
        let x = Variable::new();

        s.add_constraint(Constraint::new(
            Expression::new(
                vec![Term {
                    variable: x.clone(),
                    coefficient: 1.0,
                }],
                -10.0,
            ),
            RelOp::Equal,
            Strength::REQUIRED,
        ))
        .unwrap();

        let result = s.add_constraint(Constraint::new(
            Expression::new(
                vec![Term {
                    variable: x.clone(),
                    coefficient: 1.0,
                }],
                -20.0,
            ),
            RelOp::Equal,
            Strength::REQUIRED,
        ));
        assert_eq!(result, Err(SolverError::UnsatisfiableConstraint));
    }

    /// 4. Proportional: a == 2*b, a + b == 300  →  a=200, b=100.
    #[test]
    fn proportional_constraints() {
        let mut s = Solver::new();
        let a = Variable::new();
        let b = Variable::new();

        // a - 2*b == 0
        s.add_constraint(Constraint::new(
            Expression::new(
                vec![
                    Term {
                        variable: a.clone(),
                        coefficient: 1.0,
                    },
                    Term {
                        variable: b.clone(),
                        coefficient: -2.0,
                    },
                ],
                0.0,
            ),
            RelOp::Equal,
            Strength::REQUIRED,
        ))
        .unwrap();
        // a + b - 300 == 0
        s.add_constraint(Constraint::new(
            Expression::new(
                vec![
                    Term {
                        variable: a.clone(),
                        coefficient: 1.0,
                    },
                    Term {
                        variable: b.clone(),
                        coefficient: 1.0,
                    },
                ],
                -300.0,
            ),
            RelOp::Equal,
            Strength::REQUIRED,
        ))
        .unwrap();

        s.update_variables();
        assert!(approx(s.value_of(&a), 200.0), "a={}", s.value_of(&a));
        assert!(approx(s.value_of(&b), 100.0), "b={}", s.value_of(&b));
    }

    /// 5. Anchoring: 50 <= x <= 200, suggest x=75 → value in [50,200] ≈ 75.
    #[test]
    fn anchoring_with_bounds() {
        let mut s = Solver::new();
        let x = Variable::new();

        // x - 50 >= 0
        s.add_constraint(Constraint::new(
            Expression::new(
                vec![Term {
                    variable: x.clone(),
                    coefficient: 1.0,
                }],
                -50.0,
            ),
            RelOp::GreaterThanOrEq,
            Strength::REQUIRED,
        ))
        .unwrap();
        // x - 200 <= 0
        s.add_constraint(Constraint::new(
            Expression::new(
                vec![Term {
                    variable: x.clone(),
                    coefficient: 1.0,
                }],
                -200.0,
            ),
            RelOp::LessThanOrEq,
            Strength::REQUIRED,
        ))
        .unwrap();

        s.add_edit_variable(&x, Strength::STRONG).unwrap();
        s.suggest_value(&x, 75.0).unwrap();
        s.update_variables();

        let v = s.value_of(&x);
        assert!(v >= 50.0 - 1e-4, "x={v} < 50");
        assert!(v <= 200.0 + 1e-4, "x={v} > 200");
        assert!(approx(v, 75.0), "x={v} ≠ 75");
    }

    /// 6. Remove constraint then re-solve.
    #[test]
    fn remove_constraint_and_resolv() {
        let mut s = Solver::new();
        let x = Variable::new();

        let c1 = Constraint::new(
            Expression::new(
                vec![Term {
                    variable: x.clone(),
                    coefficient: 1.0,
                }],
                -10.0,
            ),
            RelOp::Equal,
            Strength::REQUIRED,
        );
        let c2 = Constraint::new(
            Expression::new(
                vec![Term {
                    variable: x.clone(),
                    coefficient: 1.0,
                }],
                -20.0,
            ),
            RelOp::Equal,
            Strength::REQUIRED,
        );

        s.add_constraint(c1.clone()).unwrap();
        s.update_variables();
        assert!(
            approx(s.value_of(&x), 10.0),
            "before remove: x={}",
            s.value_of(&x)
        );

        s.remove_constraint(&c1).unwrap();
        s.add_constraint(c2).unwrap();
        s.update_variables();
        assert!(
            approx(s.value_of(&x), 20.0),
            "after remove: x={}",
            s.value_of(&x)
        );
    }

    /// 7. Strength ordering: REQUIRED beats STRONG.
    #[test]
    fn strength_ordering() {
        let mut s = Solver::new();
        let x = Variable::new();

        // REQUIRED: x == 100
        s.add_constraint(Constraint::new(
            Expression::new(
                vec![Term {
                    variable: x.clone(),
                    coefficient: 1.0,
                }],
                -100.0,
            ),
            RelOp::Equal,
            Strength::REQUIRED,
        ))
        .unwrap();
        // STRONG: x == 200 (will lose to REQUIRED)
        s.add_constraint(Constraint::new(
            Expression::new(
                vec![Term {
                    variable: x.clone(),
                    coefficient: 1.0,
                }],
                -200.0,
            ),
            RelOp::Equal,
            Strength::STRONG,
        ))
        .unwrap();

        s.update_variables();
        assert!(approx(s.value_of(&x), 100.0), "x={}", s.value_of(&x));
    }

    /// 8. Reset: after reset, add x==5 and check value.
    #[test]
    fn reset_and_readd() {
        let mut s = Solver::new();
        let x = Variable::new();

        s.add_constraint(Constraint::new(
            Expression::new(
                vec![Term {
                    variable: x.clone(),
                    coefficient: 1.0,
                }],
                -99.0,
            ),
            RelOp::Equal,
            Strength::REQUIRED,
        ))
        .unwrap();
        s.update_variables();
        assert!(approx(s.value_of(&x), 99.0));

        s.reset();

        s.add_constraint(Constraint::new(
            Expression::new(
                vec![Term {
                    variable: x.clone(),
                    coefficient: 1.0,
                }],
                -5.0,
            ),
            RelOp::Equal,
            Strength::REQUIRED,
        ))
        .unwrap();
        s.update_variables();
        assert!(approx(s.value_of(&x), 5.0), "x={}", s.value_of(&x));
    }

    // ── Property-based tests ─────────────────────────────────────────────

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn no_panic_random_constraints(
            vals in prop::collection::vec(
                (0.0f64..100.0, 0.0f64..100.0), 1..10
            )
        ) {
            let mut s = Solver::new();
            let x = Variable::new();
            for (lo, hi) in &vals {
                let lo = lo.min(*hi);
                let hi = hi.max(lo);
                let _ = s.add_constraint(Constraint::new(
                    Expression::new(
                        vec![Term { variable: x.clone(), coefficient: 1.0 }],
                        -lo,
                    ),
                    RelOp::GreaterThanOrEq,
                    Strength::WEAK,
                ));
                let _ = s.add_constraint(Constraint::new(
                    Expression::new(
                        vec![Term { variable: x.clone(), coefficient: 1.0 }],
                        -hi,
                    ),
                    RelOp::LessThanOrEq,
                    Strength::WEAK,
                ));
            }
            s.update_variables();
        }
    }
}
