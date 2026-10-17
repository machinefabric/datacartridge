import Lungo
import Datacartridge.Forecast

/-!
# Trying a way of forecasting on history (specification §18, hypothesis engine)

A way of forecasting that is a rule, not a person's judgement, can be tried on the past:
at moments through history it says how likely a statement is from what was known then,
and each such forecast settles on what became known afterwards. Whoever made the rule
knows how history went, so a rule tuned on the past is only judged on a stretch it was
not tuned on.

The moments, in time order, are cut into stretches of nearly equal length. Each but the
first is a test stretch: the ways of forecasting are tried on the forecasts made before
it that had settled by the time it began, the one that did best there is chosen, and it
is scored on the test stretch. Scores are Brier scores in basis points (`Datacartridge.Forecast.brier`),
lower being better. Times are integers in whatever unit the caller counts them.
-/
namespace Datacartridge.Hindcast

open Datacartridge.Forecast (brier)

/-- Where stretch `i` begins among `n` moments cut into `k + 1` stretches. -/
def cut (n k i : Nat) : Nat := i * n / (k + 1)

/-- Stretches follow one another: a later one never begins before an earlier one. -/
@[lungo_claim "lungo.law" subject Datacartridge.Hindcast.cut]
theorem cut_in_order (n k i j : Nat) (h : i ≤ j) : cut n k i ≤ cut n k j :=
  Nat.div_le_div_right (Nat.mul_le_mul_right n h)

/-- The first stretch begins at the first moment and the last ends at the last: every
moment is in one. -/
@[lungo_claim "lungo.law" subject Datacartridge.Hindcast.cut]
theorem cuts_cover (n k : Nat) : cut n k 0 = 0 ∧ cut n k (k + 1) = n := by
  refine ⟨by simp [cut], ?_⟩
  simp only [cut]
  rw [Nat.mul_comm]
  exact Nat.mul_div_cancel n (Nat.succ_pos k)

/-- A forecast made at `madeAt` that settles `horizon` later can be learned from for a
test stretch beginning at `testStart` only if it had settled by then. -/
def learnable (madeAt horizon testStart : Int) : Bool := decide (madeAt + horizon ≤ testStart)

/-- Nothing learned from had an outcome that was not yet known when the test began. -/
@[lungo_claim "lungo.law" subject Datacartridge.Hindcast.learnable]
theorem learned_was_settled (madeAt horizon testStart : Int) (h : learnable madeAt horizon testStart = true) :
    madeAt + horizon ≤ testStart := by
  simpa [learnable] using h

/-! ## Whether history is long enough

Each stretch, the first one only learned from included, holds at least so many moments,
or its score says little. -/

def atLeastPerStretch : Nat := 5

/-- How many moments testing on so many stretches needs. -/
def needed (stretches : Nat) : Nat := (stretches + 1) * atLeastPerStretch

def enough (moments stretches : Nat) : Bool := decide (needed stretches ≤ moments)

/-! ## Whether a trial can be tried

It can when, at moments said at these times (in order), the first test stretch has
something to learn from: the first moment's forecast has settled by the time it begins.
Stretches begin in order (`cut_in_order`), so every later one has it to learn from too. -/

def triable (times : List Int) (horizon : Int) (stretches : Nat) : Bool :=
  enough times.length stretches &&
    match times.head?, times[cut times.length stretches 1]? with
    | some first, some begins => learnable first horizon begins
    | _, _ => false

/-- The most stretches a trial at these times can be tried on: none when not even one. -/
def mostTriable (times : List Int) (horizon : Int) : Option Nat :=
  ((List.range (times.length / atLeastPerStretch)).reverse).find? (fun k => decide (1 ≤ k) && triable times horizon k)

/-- It can be tried on the most stretches it is said to allow. -/
@[lungo_claim "lungo.law" subject Datacartridge.Hindcast.mostTriable Datacartridge.Hindcast.triable]
theorem most_triable_is_triable (times : List Int) (horizon : Int) (k : Nat) (h : mostTriable times horizon = some k) :
    triable times horizon k = true ∧ 1 ≤ k := by
  unfold mostTriable at h
  have := List.find?_some h
  simp only [Bool.and_eq_true, decide_eq_true_eq] at this
  exact ⟨this.2, this.1⟩

/-- One forecast tried: the likelihood given, in basis points, and whether it came true. -/
structure Tried where
  statedBps : Nat
  cameTrue : Bool
  deriving Repr, DecidableEq

/-- The forecasts' scores added up. -/
def total (tried : List Tried) : Nat := tried.foldl (fun sum t => sum + brier t.statedBps t.cameTrue) 0

/-- Their mean score; none for no forecasts. -/
def mean (tried : List Tried) : Option Nat :=
  if tried.length = 0 then none else some (total tried / tried.length)

/-- The lowest of some totals. -/
private def lowest : List Nat → Option Nat
  | [] => none
  | s :: ss => some (ss.foldl min s)

theorem foldl_min_le (ss : List Nat) (s : Nat) : ∀ x ∈ s :: ss, ss.foldl min s ≤ x := by
  induction ss generalizing s with
  | nil => simp
  | cons y ys ih =>
    intro x hx
    simp only [List.foldl_cons]
    rcases List.mem_cons.mp hx with rfl | hx
    · exact Nat.le_trans (ih (min x y) (min x y) List.mem_cons_self) (Nat.min_le_left _ _)
    · rcases List.mem_cons.mp hx with rfl | hx
      · exact Nat.le_trans (ih (min s x) (min s x) List.mem_cons_self) (Nat.min_le_right _ _)
      · exact ih (min s y) x (List.mem_cons_of_mem _ hx)

theorem foldl_min_mem (ss : List Nat) (s : Nat) : ss.foldl min s ∈ s :: ss := by
  induction ss generalizing s with
  | nil => simp
  | cons y ys ih =>
    simp only [List.foldl_cons]
    rcases List.mem_cons.mp (ih (min s y)) with h | h
    · rw [h]
      rcases Nat.le_total s y with hl | hl
      · rw [Nat.min_eq_left hl]; exact List.mem_cons_self
      · rw [Nat.min_eq_right hl]; exact List.mem_cons_of_mem _ List.mem_cons_self
    · exact List.mem_cons_of_mem _ (List.mem_cons_of_mem _ h)

/-- Of ways of forecasting with these training totals, the one chosen: the first of
those that did best. None when there is nothing to choose from. -/
def chosen (totals : List Nat) : Option Nat :=
  (lowest totals).map (fun low => totals.idxOf low)

/-- The way chosen did at least as well in training as every other. -/
@[lungo_claim "lungo.law" subject Datacartridge.Hindcast.chosen]
theorem chosen_did_best (totals : List Nat) (i : Nat) (h : chosen totals = some i) :
    ∃ t, totals[i]? = some t ∧ ∀ x ∈ totals, t ≤ x := by
  cases totals with
  | nil => simp [chosen, lowest] at h
  | cons s ss =>
    simp only [chosen, lowest, Option.map_some, Option.some.injEq] at h
    subst h
    have mem := foldl_min_mem ss s
    refine ⟨ss.foldl min s, ?_, foldl_min_le ss s⟩
    rw [List.getElem?_eq_getElem (List.idxOf_lt_length_of_mem mem)]
    simp [List.getElem_idxOf (List.idxOf_lt_length_of_mem mem)]

/-! ## Ways that look at how things stand

A base rate counts every stretch of history alike. Other ways count only the stretches
that began when things stood as they stand now: after going the same way over the last
so many days, or on the same side of the average of the last so many days. How things
stand is a signed number (a change, a distance from an average); two stand alike when
their signs are the same, nothing being a sign of its own. -/

/-- Whether a stretch began as things stand now. -/
def alike (now past : Int) : Bool := compare now 0 == compare past 0

@[lungo_claim "lungo.equals" subject Datacartridge.Hindcast.alike]
theorem alike_symm (a b : Int) : alike a b = alike b a := by
  simp only [alike]
  cases compare a 0 <;> cases compare b 0 <;> rfl

/-- Up is never alike down. -/
@[lungo_claim "lungo.equals" subject Datacartridge.Hindcast.alike]
theorem up_is_not_down (a b : Int) (ha : 0 < a) (hb : b < 0) : alike a b = false := by
  have h1 : compare a 0 = .gt := Int.compare_eq_gt.mpr ha
  have h2 : compare b 0 = .lt := Int.compare_eq_lt.mpr hb
  simp [alike, h1, h2]

/-- The likelihood such a way gives: the base rate over the stretches that began alike. -/
def alikeRate (now : Int) (stretches : List (Int × Bool)) : Option Nat :=
  let matching := stretches.filter (fun s => alike now s.1)
  Datacartridge.Forecast.baseRate (matching.filter (·.2)).length matching.length

/-! ## Choosing a kept way again

A way kept from a trial is fixed at what that trial chose. Once enough of its askings
have settled for its record going forward to say something, its keeper is offered to run
the trial again on the history known by then. -/

/-- How many settled askings a kept way's record needs before choosing again is offered. -/
def settledToChooseAgain : Nat := 5

def chooseAgain (active : Bool) (settled : Nat) : Bool := active && decide (settledToChooseAgain ≤ settled)

/-- Not offered before its record going forward says anything. -/
@[lungo_claim "lungo.law" subject Datacartridge.Hindcast.settledToChooseAgain Datacartridge.Hindcast.chooseAgain]
theorem not_too_soon (active : Bool) (settled : Nat) (h : settled < settledToChooseAgain) : chooseAgain active settled = false := by
  simp [chooseAgain]
  omega

/-- Not offered for a question no longer asked. -/
@[lungo_claim "lungo.equals" subject Datacartridge.Hindcast.chooseAgain]
theorem not_when_stopped (settled : Nat) : chooseAgain false settled = false := by
  simp [chooseAgain]

/-! ## How a kept way holds up

Set beside saying 50% every time, a kept way's record going forward says whether it
holds up, once enough of its askings have settled to say anything. -/

inductive HeldUp where
  | tooSoon
  | betterThanEven
  | noBetter
  deriving Repr, DecidableEq

/-- How a kept way with this many settled askings, scoring `forward` going forward, holds up
against saying 50% every time, which scores `even`. Scores are Brier, lower being better. -/
def heldUp (settled forward even : Nat) : HeldUp :=
  if settled < settledToChooseAgain then .tooSoon
  else if forward < even then .betterThanEven
  else .noBetter

/-- Nothing is said of a record too short to say anything. -/
@[lungo_claim "lungo.law" subject Datacartridge.Hindcast.settledToChooseAgain Datacartridge.Hindcast.heldUp]
theorem too_soon_to_tell (settled forward even : Nat) (h : settled < settledToChooseAgain) :
    heldUp settled forward even = .tooSoon := by
  simp [heldUp, h]

/-- A way said to do better than 50% scored better than it. -/
@[lungo_claim "lungo.law" subject Datacartridge.Hindcast.heldUp]
theorem better_means_lower (settled forward even : Nat) (h : heldUp settled forward even = .betterThanEven) :
    forward < even := by
  unfold heldUp at h
  split at h
  · cases h
  · split at h
    · assumption
    · cases h

/-- Whether a kept way has stopped holding up: it did better than 50% before an asking
settled and no better after. Its keeper is told then, and only then. -/
def turned (before after : HeldUp) : Bool := before == .betterThanEven && after == .noBetter

@[lungo_claim "lungo.decides" subject Datacartridge.Hindcast.turned]
theorem turned_iff (before after : HeldUp) : turned before after = true ↔ before = .betterThanEven ∧ after = .noBetter := by
  cases before <;> cases after <;> simp [turned]

/-- Nothing is told of a way while its record is too short to say anything. -/
@[lungo_claim "lungo.equals" subject Datacartridge.Hindcast.turned]
theorem not_turned_while_too_soon (before : HeldUp) : turned before .tooSoon = false := by
  cases before <;> rfl

/-! ## A person beside a kept way

The same question may be asked of a person, at their standing answer, and of a way kept
from a trial. Their records are set side by side over the stretch both were being asked:
from the later of the two first askings. Times are integers in whatever unit the caller
counts them. -/

def sharedFrom (first other : Int) : Int := max first other

/-- Whether an asking made at `madeAt` counts in a comparison that begins `from_`. -/
def countsBeside (from_ madeAt : Int) : Bool := decide (from_ ≤ madeAt)

/-- An asking counted beside the other was made when both were being asked. -/
@[lungo_claim "lungo.law" subject Datacartridge.Hindcast.countsBeside Datacartridge.Hindcast.sharedFrom]
theorem counted_when_both_were_asked (first other madeAt : Int) (h : countsBeside (sharedFrom first other) madeAt = true) :
    first ≤ madeAt ∧ other ≤ madeAt := by
  simp only [countsBeside, sharedFrom] at h
  exact Int.max_le.mp (of_decide_eq_true h)

/-- The stretch is the same seen from either side. -/
@[lungo_claim "lungo.law" subject Datacartridge.Hindcast.sharedFrom]
theorem shared_from_either_side (first other : Int) : sharedFrom first other = sharedFrom other first := by
  simp [sharedFrom, Int.max_comm]

/-- The stretch several questions asking the same were all being asked: from the latest of
their first askings. -/
def sharedFromAll (first : Int) (others : List Int) : Int := others.foldl sharedFrom first

/-- Every one of them was being asked from the start of the shared stretch on. -/
@[lungo_claim "lungo.law" subject Datacartridge.Hindcast.sharedFromAll]
theorem all_were_asked_in_the_shared_stretch : ∀ (others : List Int) (first : Int),
    first ≤ sharedFromAll first others ∧ ∀ x ∈ others, x ≤ sharedFromAll first others
  | [], first => by simp [sharedFromAll]
  | y :: ys, first => by
    obtain ⟨h1, h2⟩ := all_were_asked_in_the_shared_stretch ys (sharedFrom first y)
    have e : sharedFromAll first (y :: ys) = sharedFromAll (sharedFrom first y) ys := rfl
    have m1 : first ≤ sharedFrom first y := Int.le_max_left first y
    have m2 : y ≤ sharedFrom first y := Int.le_max_right first y
    rw [e]
    exact ⟨Int.le_trans m1 h1, fun x hx => by
      cases hx with
      | head => exact Int.le_trans m2 h1
      | tail _ hx => exact h2 x hx⟩

/-- For two, it is the stretch both were asked. -/
@[lungo_claim "lungo.equals" subject Datacartridge.Hindcast.sharedFromAll]
theorem shared_from_all_of_two (first other : Int) : sharedFromAll first [other] = sharedFrom first other := by
  simp [sharedFromAll]

/-- Whether one record set side by side goes before another: one with a mean score before
one with none yet, and of two with scores the lower, which called it better. -/
def callsBetter (mine theirs : Option Nat) : Bool :=
  match mine, theirs with
  | some a, some b => decide (a < b)
  | some _, none => true
  | none, _ => false

/-- No record goes before itself. -/
@[lungo_claim "lungo.law" subject Datacartridge.Hindcast.callsBetter]
theorem calls_better_is_strict (x : Option Nat) : callsBetter x x = false := by
  cases x <;> simp [callsBetter]

/-- Of two, at most one goes first. -/
@[lungo_claim "lungo.law" subject Datacartridge.Hindcast.callsBetter]
theorem calls_better_one_way (x y : Option Nat) (h : callsBetter x y = true) : callsBetter y x = false := by
  cases x <;> cases y <;> simp_all [callsBetter] <;> omega

/-- A record with a score goes before one with none yet. -/
@[lungo_claim "lungo.equals" subject Datacartridge.Hindcast.callsBetter]
theorem scored_goes_first (a : Nat) : callsBetter (some a) none = true := by
  simp [callsBetter]

/-! ## Each kind read back from its number

The generated code passes a kind by its number; read back, it is the kind it was. -/

@[lungo_claim "lungo.roundtrip" subject Datacartridge.Hindcast.HeldUp.ofNat]
theorem HeldUp.ofNat_reads_back (h : HeldUp) : HeldUp.ofNat h.ctorIdx = h := HeldUp.ofNat_ctorIdx h

/-! ## Scores and how much history a trial needs -/

/-- A stretch holds at least one moment. -/
@[lungo_claim "lungo.law" subject Datacartridge.Hindcast.atLeastPerStretch]
theorem a_stretch_holds_something : 0 < atLeastPerStretch := by decide

/-- Each stretch more needs as many moments more. -/
@[lungo_claim "lungo.equals" subject Datacartridge.Hindcast.needed]
theorem needed_grows_by_a_stretch (stretches : Nat) : needed (stretches + 1) = needed stretches + atLeastPerStretch := by
  simp [needed, Nat.add_mul]

@[lungo_claim "lungo.decides" subject Datacartridge.Hindcast.enough]
theorem enough_iff (moments stretches : Nat) : enough moments stretches = true ↔ (stretches + 1) * atLeastPerStretch ≤ moments := by
  simp only [enough, needed]
  exact decide_eq_true_iff

/-- No forecasts add up to nothing and have no mean. -/
@[lungo_claim "lungo.equals" subject Datacartridge.Hindcast.total]
theorem total_of_nothing : total [] = 0 := rfl

/-- One forecast more adds its score. -/
@[lungo_claim "lungo.equals" subject Datacartridge.Hindcast.total]
theorem total_of_one_more (tried : List Tried) (t : Tried) : total (tried ++ [t]) = total tried + brier t.statedBps t.cameTrue := by
  simp [total, List.foldl_append]

@[lungo_claim "lungo.equals" subject Datacartridge.Hindcast.mean]
theorem mean_of_nothing : mean [] = none := rfl

/-- A stretch counted alike is one that began as things stand: with none such, nothing is said. -/
@[lungo_claim "lungo.equals" subject Datacartridge.Hindcast.alikeRate]
theorem alike_rate_of_none_alike (now : Int) (stretches : List (Int × Bool)) (h : ∀ s ∈ stretches, alike now s.1 = false) :
    alikeRate now stretches = none := by
  have : stretches.filter (fun s => alike now s.1) = [] := List.filter_eq_nil_iff.mpr (by simpa using h)
  simp [alikeRate, this, Datacartridge.Forecast.baseRate]

/-! ## Which ways can speak for a horizon

A way that says how often the statement held over the last so many days counts only
statements already settled: made at least the horizon before. Over a stretch no longer
than the horizon none has settled, so such a way could never say anything, and a moment
is tried only when every way speaks. It is not one of the ways tried for that horizon.
The other ways look back at the series itself, and speak for any horizon. -/

/-- Whether a way counting how often the statement held (`often`), over `days` (none for
all history known), can speak for statements looking `horizon` days ahead. -/
def speaks (often : Bool) (days : Option Nat) (horizon : Nat) : Bool :=
  !often || match days with
    | none => true
    | some d => decide (horizon < d)

/-- Counting over all history known always speaks. -/
@[lungo_claim "lungo.law" subject Datacartridge.Hindcast.speaks]
theorem all_history_speaks (horizon : Nat) : speaks true none horizon = true := rfl

/-- Counting over no more days than it looks ahead never speaks. -/
@[lungo_claim "lungo.law" subject Datacartridge.Hindcast.speaks]
theorem too_short_a_count_is_silent (d horizon : Nat) (h : d ≤ horizon) : speaks true (some d) horizon = false := by
  simp [speaks]
  omega

/-- A way looking at the series itself speaks for any horizon. -/
@[lungo_claim "lungo.law" subject Datacartridge.Hindcast.speaks]
theorem the_series_itself_speaks (days : Option Nat) (horizon : Nat) : speaks false days horizon = true := rfl

/-! ## A way that cannot say anything at a moment

A way may have nothing to go by at a moment: too little history behind it, or a series
it cannot read (swings need values above nothing). It is taken there to say 50%: it knows
nothing, and is scored as knowing nothing. A moment is left out only when not one way
can say anything; otherwise one silent way would leave every moment out. Likelihoods
are in basis points. -/

/-- What a way says at a moment: what it said, or 50% when it could say nothing. -/
def said (stated : Option Nat) : Nat := stated.getD 5000

/-- What the ways say at a moment, in their order, or none when not one could say anything. -/
def moment (stated : List (Option Nat)) : Option (List Nat) :=
  if stated.any Option.isSome then .some (stated.map said) else .none

/-- A way that could say nothing says 50%. -/
@[lungo_claim "lungo.equals" subject Datacartridge.Hindcast.said]
theorem silent_says_even : said .none = 5000 := rfl

/-- A way that said something is taken at its word. -/
@[lungo_claim "lungo.law" subject Datacartridge.Hindcast.said]
theorem spoken_is_kept (n : Nat) : said (.some n) = n := rfl

/-- A moment is left out exactly when no way could say anything. -/
@[lungo_claim "lungo.law" subject Datacartridge.Hindcast.moment]
theorem left_out_only_when_all_silent (stated : List (Option Nat)) :
    moment stated = .none ↔ stated.all Option.isNone := by
  unfold moment
  induction stated with
  | nil => simp
  | cons s ss ih => cases s <;> simp_all

/-- Every way has its say at a moment kept: as many likelihoods as ways. -/
@[lungo_claim "lungo.law" subject Datacartridge.Hindcast.moment]
theorem every_way_is_heard (stated : List (Option Nat)) (said : List Nat) (h : moment stated = .some said) :
    said.length = stated.length := by
  unfold moment at h
  split at h
  · simp only [Option.some.injEq] at h
    subst h
    simp
  · simp at h

end Datacartridge.Hindcast
