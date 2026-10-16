import Lungo
/-!
# Forecasts (specification §21, §27)

A forecast says that something measurable will be so by a deadline, and how likely its
author holds that to be. This module decides two things and nothing else: what has
become of a forecast given what has been observed, and how far its author's stated
likelihood was from what happened.

Whether one observation satisfies the forecast ("the close was above 100,000") is plain
arithmetic on numbers and is done where the numbers are. What is decided here is what
follows from those answers.
-/
namespace Mos.Forecast

/-- When the forecast must hold. -/
inductive Window where
  /-- At the deadline: judged on the last observation at or before it. -/
  | atDeadline
  /-- At any time up to the deadline: one observation that satisfies it is enough. -/
  | anyTime
  deriving Repr, DecidableEq

inductive Outcome where
  /-- Not yet decided: the deadline has not passed and nothing has decided it early. -/
  | open
  | cameTrue
  | didNot
  /-- The deadline passed and nothing was observed to judge it by. -/
  | void
  deriving Repr, DecidableEq

/-- The last element of a list, if it has one. -/
private def last? : List Bool → Option Bool
  | [] => none
  | [x] => some x
  | _ :: rest => last? rest

/--
What has become of a forecast.

`due` is whether the deadline has passed. `observed` holds, for each observation made
after the forecast was stated and at or before its deadline, in the order of their
times, whether that observation satisfies the forecast.
-/
def settle (window : Window) (due : Bool) (observed : List Bool) : Outcome :=
  match window with
  | .anyTime =>
    if observed.any id then .cameTrue
    else if !due then .open
    else if observed.isEmpty then .void
    else .didNot
  | .atDeadline =>
    if !due then .open
    else match last? observed with
      | none => .void
      | some true => .cameTrue
      | some false => .didNot

/-- A forecast is settled once its outcome is anything but open. -/
def settled (o : Outcome) : Bool :=
  match o with
  | .open => false
  | _ => true

/-! ## How a forecast is settled, as one contract

Everything `settle` promises, said once and without reference to how it does it: a forecast
is open exactly while its deadline has not passed and nothing has decided it early; one that
may hold at any time came true exactly when something seen satisfied it; one for its
deadline is decided, once due, by the last thing seen; and it is void exactly when its
deadline passed with nothing seen. -/

theorem last?_is_getLast? : ∀ l : List Bool, last? l = l.getLast?
  | [] => rfl
  | [_] => rfl
  | _ :: y :: rest => by
    rw [last?.eq_3 _ (y :: rest) (by simp), last?_is_getLast? (y :: rest)]
    simp

open Lungo.Spec in
@[lungo_spec "lungo.contract"]
noncomputable def Settling : Contract (Window × Bool × List Bool) Outcome where
  pre _ := True
  post input o :=
    let (window, due, observed) := input
    (o = .open ↔ (due = false ∧ (window = .atDeadline ∨ observed.any id = false))) ∧
      (window = .anyTime → (o = .cameTrue ↔ observed.any id = true)) ∧
      (window = .atDeadline → due = true → ((o = .cameTrue ↔ observed.getLast? = some true) ∧ (o = .didNot ↔ observed.getLast? = some false))) ∧
      (o = .void ↔ (due = true ∧ observed = []))

open Lungo.Spec in
@[lungo_claim "lungo.satisfies" subject Mos.Forecast.settle spec Mos.Forecast.Settling]
theorem settle_satisfies_settling : Satisfies (fun (input : Window × Bool × List Bool) => settle input.1 input.2.1 input.2.2) Settling := by
  rintro ⟨window, due, observed⟩ -
  cases window <;> cases due <;> simp only [Settling, settle, last?_is_getLast?] <;>
    cases h : observed.any id <;> rcases hl : observed.getLast? with _ | ⟨_ | _⟩ <;>
    simp_all [List.getLast?_eq_none_iff] <;>
    (try (split <;> simp_all)) <;>
    (try (intro he; simp [he] at hl))

/-- Before its deadline, a forecast that must hold at the deadline is open whatever
has been seen: nothing seen early decides it. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.settle]
theorem at_deadline_waits (observed : List Bool) : settle .atDeadline false observed = .open := by
  simp [settle]

/-- A forecast that may hold at any time is decided the moment it is seen to hold,
deadline or no. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.settle]
theorem any_time_settles_early (due : Bool) (observed : List Bool) (h : observed.any id = true) :
    settle .anyTime due observed = .cameTrue := by
  simp [settle, h]

/-- Once the deadline has passed, every forecast is settled. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.settled Mos.Forecast.settle]
theorem due_is_settled (window : Window) (observed : List Bool) : settled (settle window true observed) = true := by
  cases window with
  | anyTime =>
    by_cases h : observed.any id = true
    · simp [settle, h, settled]
    · by_cases e : observed.isEmpty = true
      · simp [settle, h, e, settled]
      · simp [settle, h, e, settled]
  | atDeadline =>
    simp only [settle]
    cases last? observed with
    | none => simp [settled]
    | some b => cases b <;> simp [settled]

/-- With nothing observed, a forecast is never judged right or wrong: it is open until
its deadline and void after. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.settle]
theorem nothing_observed (window : Window) (due : Bool) :
    settle window due [] = (if due then .void else .open) := by
  cases window <;> cases due <;> simp [settle, last?]

/-- What was seen after a forecast that may hold at any time came true does not undo it. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.settle]
theorem any_time_stays_true (due due' : Bool) (observed later : List Bool)
    (h : settle .anyTime due observed = .cameTrue) : settle .anyTime due' (observed ++ later) = .cameTrue := by
  have seen : observed.any id = true := by
    by_cases a : observed.any id = true
    · exact a
    · by_cases d : due = true <;> by_cases e : observed.isEmpty = true <;> simp [settle, a, d, e] at h
  simp [settle, List.any_append, seen]

/-! ## A figure that sources disagree on

A figure can be reported by more than one source, and they can differ. A forecast that
names its source is judged on that source alone. One that names none is judged on what
the sources agree on: a moment at which one source's figure satisfies it and another's
does not is no observation either way. If that is how things stand at the deadline, a
forecast for the deadline cannot be judged; it is not right by one and wrong by another. -/

/--
What has become of a forecast about a reported figure. `observed` holds, in order, the
moments at which the sources agreed and whether they agreed it was satisfied;
`contestedAtEnd` is whether, as last reported, they disagree.
-/
def settleReported (window : Window) (due : Bool) (observed : List Bool) (contestedAtEnd : Bool) : Outcome :=
  match window with
  | .atDeadline => if due && contestedAtEnd then .void else settle .atDeadline due observed
  | .anyTime => settle .anyTime due observed

open Lungo.Spec in
/-- A reported figure is settled as any measurement is, except that sources disagreeing at
the deadline leave a forecast for the deadline void. -/
@[lungo_spec "lungo.contract"]
noncomputable def ReportedSettling : Contract (Window × Bool × List Bool × Bool) Outcome where
  pre _ := True
  post input o :=
    let (window, due, observed, contested) := input
    (window = .atDeadline ∧ due = true ∧ contested = true → o = .void) ∧
      (¬ (window = .atDeadline ∧ due = true ∧ contested = true) → Settling.post (window, due, observed) o)

open Lungo.Spec in
@[lungo_claim "lungo.satisfies" subject Mos.Forecast.settleReported spec Mos.Forecast.ReportedSettling]
theorem settle_reported_satisfies :
    Satisfies (fun (input : Window × Bool × List Bool × Bool) => settleReported input.1 input.2.1 input.2.2.1 input.2.2.2) ReportedSettling := by
  rintro ⟨window, due, observed, contested⟩ -
  have usual := settle_satisfies_settling (window, due, observed) trivial
  cases window <;> cases due <;> cases contested <;> simp_all [ReportedSettling, settleReported]

/-- With nothing contested it is judged as any measurement is. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.settleReported Mos.Forecast.settle]
theorem agreed_is_settled_as_usual (window : Window) (due : Bool) (observed : List Bool) :
    settleReported window due observed false = settle window due observed := by
  cases window <;> simp [settleReported]

/-- Sources that disagree at the deadline leave a forecast for the deadline unjudged. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.settleReported]
theorem contested_at_the_deadline_is_void (observed : List Bool) : settleReported .atDeadline true observed true = .void := by
  simp [settleReported]

/-- A moment of agreement that it held is enough for a forecast that needed only one,
whatever the sources say now. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.settleReported]
theorem any_time_is_not_undone_by_a_later_dispute (due contested : Bool) (observed : List Bool) (h : observed.any id = true) :
    settleReported .anyTime due observed contested = .cameTrue := by
  simp [settleReported, settle, h]

/-! ## Something happening

A forecast that something will happen is not about a number that may or may not have
been measured: it is about whether anything of the kind became known. One such thing is
enough. None by the deadline is an answer too, but only where it could have been heard:
silence from an installation that some source tells of such things is evidence that
it did not happen; silence from one that no source has ever told of such a thing is
only silence. -/

/-- What the installation has ever been told of events of the kind. -/
inductive Hearing where
  /-- A source has reported such an event about this very entity before. -/
  | ofThis
  /-- Sources report such events, though none yet about this entity. -/
  | ofOthers
  /-- No source has ever reported such an event about anything. -/
  | never
  deriving Repr, DecidableEq

/-- Whether hearing nothing says anything. -/
def silenceCounts (h : Hearing) : Bool :=
  match h with
  | .never => false
  | _ => true

/-- What has become of a forecast that something happens, from whether the deadline has
passed, whether anything of the kind has become known since it was stated, and what
the installation hears of such things at all. -/
def occurs (due seen : Bool) (hearing : Hearing) : Outcome :=
  if seen then .cameTrue
  else if !due then .open
  else if silenceCounts hearing then .didNot
  else .void

@[lungo_claim "lungo.equals" subject Mos.Forecast.settled Mos.Forecast.occurs]
theorem occurs_settles_at_the_deadline (seen : Bool) (hearing : Hearing) : settled (occurs true seen hearing) = true := by
  cases seen <;> cases hearing <;> decide

/-- Once it has happened, nothing else matters. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.occurs]
theorem occurs_stays_true (due : Bool) (hearing : Hearing) : occurs due true hearing = .cameTrue := by
  cases due <;> cases hearing <;> decide

/-- It is called wrong only where it could have been heard. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.occurs Mos.Forecast.silenceCounts]
theorem wrong_only_where_it_could_be_heard (due seen : Bool) (hearing : Hearing) (h : occurs due seen hearing = .didNot) :
    silenceCounts hearing = true := by
  cases due <;> cases seen <;> cases hearing <;> simp_all [occurs, silenceCounts]

/-- Where nothing of the kind has ever been heard, silence at the deadline leaves it unjudged. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.occurs]
theorem unheard_silence_is_void : occurs true false .never = .void := by decide

/-! ## Statements made of several

A hypothesis is often more than one number. Parts are settled one by one as above and
combined here. A part that is void was never observed: it neither confirms nor refutes,
so a whole that depends on it cannot be judged either, unless another part already
decides the matter. -/

/-- All of the parts: refuted by one that did not, confirmed only by every one. -/
def all (parts : List Outcome) : Outcome :=
  if parts.any (· == .didNot) then .didNot
  else if parts.all (· == .cameTrue) then .cameTrue
  else if parts.any (· == .open) then .open
  else .void

/-- Any of the parts: confirmed by one that did, refuted only by every one. -/
def any (parts : List Outcome) : Outcome :=
  if parts.any (· == .cameTrue) then .cameTrue
  else if parts.any (· == .open) then .open
  else if parts.any (· == .void) then .void
  else .didNot

/--
"If the first happens, then the second." A condition that never happened says nothing
about what would have followed, so the whole is void, not wrong. Once the condition
has happened the whole is whatever becomes of what was to follow.
-/
def conditional (condition consequence : Outcome) : Outcome :=
  match condition with
  | .open => .open
  | .cameTrue => consequence
  | .didNot => .void
  | .void => .void

@[lungo_claim "lungo.equals" subject Mos.Forecast.all]
theorem all_of_one (o : Outcome) : all [o] = o := by cases o <;> decide
@[lungo_claim "lungo.equals" subject Mos.Forecast.any]
theorem any_of_one (o : Outcome) : any [o] = o := by cases o <;> decide

/-- One part that did not come true refutes the whole, whatever the rest are doing. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.all]
theorem all_refuted (parts : List Outcome) (h : .didNot ∈ parts) : all parts = .didNot := by
  have : parts.any (· == .didNot) = true := List.any_eq_true.mpr ⟨.didNot, h, by decide⟩
  simp [all, this]

/-- One part that came true confirms the whole, whatever the rest are doing. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.any]
theorem any_confirmed (parts : List Outcome) (h : .cameTrue ∈ parts) : any parts = .cameTrue := by
  have : parts.any (· == .cameTrue) = true := List.any_eq_true.mpr ⟨.cameTrue, h, by decide⟩
  simp [any, this]

/-- A conditional statement is never judged wrong because its condition failed. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.conditional]
theorem conditional_is_not_refuted_by_its_condition (consequence : Outcome) :
    conditional .didNot consequence = .void := rfl

/-- When every part is settled, so is the whole. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.settled Mos.Forecast.all]
theorem all_settles (parts : List Outcome) (h : ∀ o ∈ parts, settled o = true) : settled (all parts) = true := by
  have none_open : parts.any (· == .open) = false := by
    rw [List.any_eq_false]
    intro o ho
    have := h o ho
    cases o <;> simp_all [settled]
  unfold all
  by_cases a : parts.any (· == .didNot) = true
  · simp [a, settled]
  · by_cases b : parts.all (· == .cameTrue) = true
    · simp [a, b, settled]
    · simp [a, b, none_open, settled]

@[lungo_claim "lungo.equals" subject Mos.Forecast.settled Mos.Forecast.any]
theorem any_settles (parts : List Outcome) (h : ∀ o ∈ parts, settled o = true) : settled (any parts) = true := by
  have none_open : parts.any (· == .open) = false := by
    rw [List.any_eq_false]
    intro o ho
    have := h o ho
    cases o <;> simp_all [settled]
  unfold any
  by_cases a : parts.any (· == .cameTrue) = true
  · simp [a, settled]
  · by_cases b : parts.any (· == .void) = true
    · simp [a, b, none_open, settled]
    · simp [a, b, none_open, settled]

/-! ## Scoring

A stated likelihood is in basis points: 0 is "cannot happen", 10000 is "certain". The
score of one forecast is the square of the distance between what was said and what
happened (the Brier score), also in basis points: 0 is a certain forecast that was
right, 10000 a certain one that was wrong, and 2500 is what saying "even odds" always
earns. Lower is better. -/

/-- A likelihood above certainty counts as certainty. -/
def likelihood (bps : Nat) : Nat := min bps 10000

/-- The Brier score of one forecast, in basis points. -/
def brier (statedBps : Nat) (cameTrue : Bool) : Nat :=
  let p := likelihood statedBps
  let miss := if cameTrue then 10000 - p else p
  miss * miss / 10000

@[lungo_claim "lungo.law" subject Mos.Forecast.brier]
theorem brier_is_bounded (statedBps : Nat) (cameTrue : Bool) : brier statedBps cameTrue ≤ 10000 := by
  have hp : likelihood statedBps ≤ 10000 := Nat.min_le_right _ _
  have step (m : Nat) (hm : m ≤ 10000) : m * m / 10000 ≤ 10000 := by
    have : m * m ≤ 10000 * 10000 := Nat.mul_le_mul hm hm
    omega
  unfold brier
  cases cameTrue
  · simpa using step _ hp
  · simpa using step (10000 - likelihood statedBps) (by omega)

/-! ## A score split into what it is made of

A forecaster's mean Brier score mixes three things (Murphy's decomposition): how far what
they said was from how often it happened (calibration, lower is better), how much what they
said told apart what happened from what did not (discernment, higher is better), and how
hard the questions were, how mixed their outcomes (not theirs to change). Forecasts are
grouped by the likelihood said; within a group the split is exact. Scores are in basis
points. -/

/-- Forecasts that said the same likelihood: how many, and how many came true. -/
structure Group where
  said : Nat
  calls : Nat
  came : Nat
  deriving Repr, DecidableEq

/-- A group's squared misses, in basis points squared. -/
def Group.squares (g : Group) : Int :=
  let p : Int := likelihood g.said
  g.came * (10000 - p) ^ 2 + ((g.calls : Int) - g.came) * p ^ 2

/-- How far the group's likelihood was from how often it came true, times its size, squared. -/
def Group.gap (g : Group) : Int := ((g.calls : Int) * likelihood g.said - 10000 * g.came) ^ 2

/-- Within a group, its squared misses are its calibration gap and the mix of its outcomes: nothing else. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.Group.squares Mos.Forecast.Group.gap]
theorem group_split (g : Group) : (g.calls : Int) * g.squares = g.gap + 100000000 * (g.came * ((g.calls : Int) - g.came)) := by
  unfold Group.squares Group.gap
  grind

structure Split where
  /-- The mean score over these forecasts, as said when each was decided. -/
  total : Nat
  /-- How far what was said was from how often it happened. -/
  offBy : Nat
  /-- How much what was said told apart what happened from what did not. -/
  toldApart : Nat
  /-- How mixed the outcomes were: what saying the same every time would have to overcome. -/
  howHard : Nat
  deriving Repr, DecidableEq

/-- The split of a record's score; none for no forecasts. Each part is rounded down, so they
add up to the total within a few basis points. -/
def split (groups : List Group) : Option Split :=
  let n : Int := ((groups.map (·.calls)).sum : Nat)
  let o : Int := ((groups.map (·.came)).sum : Nat)
  if n = 0 then none else
  let counted := groups.filter (0 < ·.calls)
  some {
    total := ((counted.map (·.squares)).sum / n / 10000).toNat
    offBy := ((counted.map (fun g => g.gap / (g.calls : Int))).sum / n / 10000).toNat
    toldApart := ((counted.map (fun g => 10000 * (n * g.came - g.calls * o) ^ 2 / (g.calls : Int))).sum / (n * n * n)).toNat
    howHard := (10000 * o * (n - o) / (n * n)).toNat }

/-- No forecasts have no split. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.split]
theorem split_of_none (groups : List Group) (h : (groups.map (·.calls)).sum = 0) : split groups = none := by
  simp [split, h]

/-- What a split says of a record in words: calibrated when what was said was within a
hundredth (0.010) of how often it happened; discerning when it told apart at least a
quarter of what there was to tell. -/
def calibrated (s : Split) : Bool := decide (s.offBy ≤ 100)

def discerning (s : Split) : Bool := decide (0 < s.howHard) && decide (s.howHard ≤ 4 * s.toldApart)

@[lungo_claim "lungo.decides" subject Mos.Forecast.calibrated]
theorem calibrated_iff (s : Split) : calibrated s = true ↔ s.offBy ≤ 100 := by
  simp [calibrated]

@[lungo_claim "lungo.decides" subject Mos.Forecast.discerning]
theorem discerning_iff (s : Split) : discerning s = true ↔ 0 < s.howHard ∧ s.howHard ≤ 4 * s.toldApart := by
  simp [discerning]


/-- Certainty that was right scores nothing; certainty that was wrong scores everything. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.brier]
theorem brier_of_certainty : brier 10000 true = 0 ∧ brier 10000 false = 10000 ∧ brier 0 false = 0 ∧ brier 0 true = 10000 := by
  decide

/-- Even odds earn 2500 whatever happens: the score to beat. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.brier]
theorem brier_of_even_odds (cameTrue : Bool) : brier 5000 cameTrue = 2500 := by
  cases cameTrue <;> decide

/-- Saying something that happened was more likely never scores worse. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.brier]
theorem brier_rewards_being_right (p q : Nat) (h : p ≤ q) : brier q true ≤ brier p true := by
  have hl : likelihood p ≤ likelihood q := by
    unfold likelihood
    omega
  have hq : likelihood q ≤ 10000 := Nat.min_le_right _ _
  unfold brier
  simp only [ite_true]
  have : 10000 - likelihood q ≤ 10000 - likelihood p := by omega
  exact Nat.div_le_div_right (Nat.mul_le_mul this this)

/-- One likelihood a forecast stood at, and for how long (in any unit, the same throughout). -/
structure Stand where
  statedBps : Nat
  duration : Nat
  deriving Repr, DecidableEq

private def weightedSum (stands : List Stand) (cameTrue : Bool) : Nat :=
  stands.foldl (fun sum s => sum + brier s.statedBps cameTrue * s.duration) 0

private def totalDuration (stands : List Stand) : Nat :=
  stands.foldl (fun sum s => sum + s.duration) 0

/--
The score of a forecast over its whole life: each likelihood it stood at counts for as
long as it stood, so being right only at the last minute earns little. A forecast that
stood for no time at all has no score.
-/
def lifeBrier (stands : List Stand) (cameTrue : Bool) : Option Nat :=
  if totalDuration stands = 0 then none else some (weightedSum stands cameTrue / totalDuration stands)

/-- A forecast never revised scores what its one likelihood scores. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.lifeBrier Mos.Forecast.brier]
theorem life_of_one (statedBps duration : Nat) (cameTrue : Bool) (h : 0 < duration) :
    lifeBrier [⟨statedBps, duration⟩] cameTrue = some (brier statedBps cameTrue) := by
  simp [lifeBrier, totalDuration, weightedSum, Nat.pos_iff_ne_zero.mp h, Nat.mul_div_cancel _ h]

/-- Which tenth of the scale a stated likelihood falls in, 0 to 9: forecasts in the same
tenth are compared with how often they came true. Certainty goes with the nineties. -/
def tenth (statedBps : Nat) : Nat := min (likelihood statedBps / 1000) 9

@[lungo_claim "lungo.law" subject Mos.Forecast.tenth]
theorem tenth_is_bounded (statedBps : Nat) : tenth statedBps ≤ 9 := Nat.min_le_right _ _

/-- The average of scores, in basis points; nothing scored has no average. -/
def mean (scores : List Nat) : Option Nat :=
  if scores.isEmpty then none else some (scores.foldl (· + ·) 0 / scores.length)

/--
How much better than even odds an average score is, in basis points of the even-odds
score: 10000 is perfect, 0 is no better than saying "even odds" every time, and below
nothing is clamped at nothing here and reported separately as `worse`.
-/
def skill (meanBrier : Nat) : Nat := (2500 - min meanBrier 2500) * 10000 / 2500

/-- Whether an average score is worse than always saying "even odds". -/
def worse (meanBrier : Nat) : Bool := 2500 < meanBrier

@[lungo_claim "lungo.law" subject Mos.Forecast.skill]
theorem skill_is_bounded (meanBrier : Nat) : skill meanBrier ≤ 10000 := by
  unfold skill
  omega

/-! ## How much to believe a forecaster

A record of being right is worth something only once there is enough of it. Until
then a forecaster counts as anyone does. After, one who has beaten even odds counts
for more, up to twice as much, and one who has done worse than even odds counts for
less, down to a fifth; nobody counts for nothing, since a poor record can be mended. -/

/-- How many forecasts must have come true or not before a record counts. -/
def provenAt : Nat := 5

def proven (scored : Nat) : Bool := provenAt ≤ scored

/-- What an unproven forecaster counts for, in basis points: as anyone does. -/
def evenWeight : Nat := 5000

/--
What a forecaster's word counts for, in basis points, from how many of their forecasts
have been scored and their average score: `evenWeight` while unproven, rising to 10000
for a perfect record and falling to 1000 for one that could not be worse.
-/
def weight (scored meanBrier : Nat) : Nat :=
  if scored < provenAt then evenWeight
  else if meanBrier ≤ 2500 then evenWeight + skill meanBrier / 2
  else evenWeight - (min meanBrier 10000 - 2500) * 4000 / 7500

@[lungo_claim "lungo.law" subject Mos.Forecast.weight]
theorem weight_is_bounded (scored meanBrier : Nat) : 1000 ≤ weight scored meanBrier ∧ weight scored meanBrier ≤ 10000 := by
  unfold weight evenWeight
  by_cases h : scored < provenAt
  · simp [h]
  · by_cases g : meanBrier ≤ 2500
    · have := skill_is_bounded meanBrier
      simp [h, g]
      omega
    · simp [h, g]
      omega

/-- Until a record is proven it changes nothing, however good or bad it looks. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.provenAt Mos.Forecast.weight Mos.Forecast.evenWeight]
theorem unproven_counts_as_anyone (scored meanBrier : Nat) (h : scored < provenAt) : weight scored meanBrier = evenWeight := by
  simp [weight, h]

/-- Even odds, proven, still counts as anyone does: the record says nothing either way. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.weight Mos.Forecast.evenWeight]
theorem even_odds_counts_as_anyone (scored : Nat) : weight scored 2500 = evenWeight := by
  unfold weight evenWeight skill
  by_cases h : scored < provenAt <;> simp [h]

/-- Among the proven, a better average never counts for less. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.weight]
theorem better_counts_for_no_less (scored better worse : Nat) (h : better ≤ worse) :
    weight scored worse ≤ weight scored better := by
  unfold weight evenWeight skill
  by_cases p : scored < provenAt
  · simp [p]
  · by_cases a : worse ≤ 2500
    · have b : better ≤ 2500 := Nat.le_trans h a
      simp [p, a, b]
      omega
    · by_cases b : better ≤ 2500
      · simp [p, a, b]
        omega
      · simp [p, a, b]
        omega

/-! ## Several views of one statement

When more than one forecaster has said how likely a statement is, their likelihoods are
pooled: an average in which each counts for what its author's record has earned. -/

/-- One forecaster's likelihood for a statement, and what their word counts for. -/
structure View where
  weightBps : Nat
  statedBps : Nat
  deriving Repr, DecidableEq

def weights : List View → Nat
  | [] => 0
  | v :: rest => v.weightBps + weights rest

def weighted : List View → Nat
  | [] => 0
  | v :: rest => v.weightBps * likelihood v.statedBps + weighted rest

/-- The pooled likelihood, in basis points. Views that count for nothing make no pool. -/
def pool (views : List View) : Option Nat :=
  if weights views = 0 then none else some (weighted views / weights views)

@[lungo_claim "lungo.law" subject Mos.Forecast.weighted Mos.Forecast.weights]
theorem weighted_is_bounded (views : List View) : weighted views ≤ 10000 * weights views := by
  induction views with
  | nil => simp [weighted, weights]
  | cons v rest ih =>
    have hl : likelihood v.statedBps ≤ 10000 := Nat.min_le_right _ _
    have : v.weightBps * likelihood v.statedBps ≤ v.weightBps * 10000 := Nat.mul_le_mul_left _ hl
    simp only [weighted, weights]
    omega

/-- A pool is a likelihood: never beyond certainty. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.pool]
theorem pool_is_bounded (views : List View) (p : Nat) (h : pool views = some p) : p ≤ 10000 := by
  unfold pool at h
  by_cases z : weights views = 0
  · simp [z] at h
  · simp [z] at h
    subst h
    have := weighted_is_bounded views
    exact Nat.div_le_of_le_mul (by rw [Nat.mul_comm]; exact this)

/-- One view pools to itself, whatever it counts for. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.pool Mos.Forecast.likelihood]
theorem pool_of_one (weightBps statedBps : Nat) (h : 0 < weightBps) :
    pool [⟨weightBps, statedBps⟩] = some (likelihood statedBps) := by
  simp [pool, weights, weighted, Nat.pos_iff_ne_zero.mp h, Nat.mul_div_cancel_left _ h]

/-- Views that agree pool to what they agree on. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.likelihood Mos.Forecast.weights Mos.Forecast.pool]
theorem pool_of_agreement (views : List View) (statedBps : Nat) (h : ∀ v ∈ views, likelihood v.statedBps = statedBps)
    (w : 0 < weights views) : pool views = some statedBps := by
  have sum : weighted views = weights views * statedBps := by
    induction views with
    | nil => simp [weighted, weights]
    | cons v rest ih =>
      have hv := h v (List.mem_cons_self ..)
      have hr := ih (fun u hu => h u (List.mem_cons_of_mem _ hu))
      simp only [weighted, weights, hv]
      by_cases z : weights rest = 0
      · have : weighted rest = 0 := by
          have := weighted_is_bounded rest
          omega
        simp [z, this]
      · rw [hr (Nat.pos_of_ne_zero z), Nat.add_mul]
  simp [pool, Nat.pos_iff_ne_zero.mp w, sum, Nat.mul_div_cancel_left _ w]

/-! ## How often it has happened before

Before saying how likely something is, it helps to know how often the same thing has
happened: over every stretch of history as long as the forecast looks ahead, in how
many did the statement hold? That is its base rate. A handful of stretches says
little, and is shown as such. -/

/-- How many stretches of history a base rate needs before it tells anything. -/
private def tellingAt : Nat := 20

def telling (windows : Nat) : Bool := tellingAt ≤ windows

/-- The base rate in basis points: the share of stretches in which the statement held. -/
def baseRate (held windows : Nat) : Option Nat :=
  if windows = 0 then none else some (min held windows * 10000 / windows)

@[lungo_claim "lungo.law" subject Mos.Forecast.baseRate]
theorem base_rate_is_bounded (held windows r : Nat) (h : baseRate held windows = some r) : r ≤ 10000 := by
  unfold baseRate at h
  by_cases z : windows = 0
  · simp [z] at h
  · simp [z] at h
    subst h
    have : min held windows ≤ windows := Nat.min_le_right _ _
    exact Nat.div_le_of_le_mul (by omega)

/-- Held in every stretch is certainty; in none, impossibility. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.baseRate]
theorem base_rate_extremes (windows : Nat) (h : 0 < windows) :
    baseRate windows windows = some 10000 ∧ baseRate 0 windows = some 0 := by
  have z : windows ≠ 0 := Nat.pos_iff_ne_zero.mp h
  simp [baseRate, z, Nat.mul_div_cancel_left _ h]

/-- A base rate stated as a forecast: never certainty either way, as no forecast is. -/
def asForecast (rateBps : Nat) : Nat := max 100 (min rateBps 9900)

@[lungo_claim "lungo.law" subject Mos.Forecast.asForecast]
theorem as_forecast_is_a_likelihood (rateBps : Nat) : 100 ≤ asForecast rateBps ∧ asForecast rateBps ≤ 9900 := by
  unfold asForecast
  omega

/-! ## A question asked again and again

A question kept to be asked every so often is asked when its time has come, once,
however long nobody looked: an installation that was off for a month does not ask the
four weekly askings it missed, since each would be a forecast stated late about a
time partly past. It asks now, and next at the first of its regular times still ahead. -/

/-- Whether a question's time has come. Times are in seconds. -/
private def askNow (now next : Nat) : Bool := next ≤ now

/-- Whether a kept question is asked now. One that has been stopped is not, and nor is
one its keeper has not answered: asking states a forecast in the keeper's name, and
there is nothing to state until they have said what they believe. -/
def asks (active answered : Bool) (now next : Nat) : Bool := active && answered && askNow now next

/-- Nothing is stated in the name of someone who has not said what they believe. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.asks]
theorem unanswered_is_not_asked (active : Bool) (now next : Nat) : asks active false now next = false := by
  simp [asks]

/-- A question that is asked is one still kept, answered, and due. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.asks]
theorem asked_means_due (active answered : Bool) (now next : Nat) (h : asks active answered now next = true) :
    active = true ∧ answered = true ∧ next ≤ now := by
  simp [asks, askNow] at h
  exact ⟨h.1.1, h.1.2, h.2⟩

/-- When it is next to be asked after being asked at `now`: the first of its regular
times, counted from `next`, that is still ahead. -/
def askNext (now next every : Nat) : Nat :=
  if now < next then next else next + ((now - next) / every + 1) * every

/-- The next asking is always ahead of now, for a question asked every so often. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.askNext]
theorem ask_next_is_ahead (now next every : Nat) (h : 0 < every) : now < askNext now next every := by
  unfold askNext
  by_cases e : now < next
  · simp [e]
  · simp [e]
    have d := Nat.div_add_mod (now - next) every
    have m := Nat.mod_lt (now - next) h
    have : ((now - next) / every + 1) * every = every * ((now - next) / every) + every := by
      rw [Nat.add_mul, Nat.one_mul, Nat.mul_comm]
    omega

/-- It keeps to its regular times: the next asking is a whole number of intervals on. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.askNext]
theorem ask_next_keeps_time (now next every : Nat) (h : next ≤ now) : (askNext now next every - next) % every = 0 := by
  have e : ¬ now < next := by omega
  simp [askNext, e]

/-- Asked on time, the next asking is one interval on. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.askNext]
theorem ask_next_on_time (next every : Nat) : askNext next next every = next + every := by
  simp [askNext, Nat.zero_div]

/-! ## How much a forecast has behind it

What to act on first is not what is called most likely: it is what is believed to be
most different from how things usually go, by those whose word has earned most. A
forecast called 90% likely of something that happens nine times in ten says nothing
new. Its backing is how far the likelihood believed departs from the base rate, counted
for what the believer's record has earned; where no base rate is known, even odds
stand in for it. -/

/-- The backing of a forecast, in basis points: positive when it is believed more
likely than history says, negative when less. -/
def backing (believedBps : Nat) (baseBps : Option Nat) (weightBps : Nat) : Int :=
  ((likelihood believedBps : Int) - (likelihood (baseBps.getD 5000) : Int)) * (weightBps : Int) / (evenWeight : Int)

/-- Believing what history says has nothing behind it, whoever believes it. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.backing]
theorem no_backing_without_a_difference (bps weightBps : Nat) : backing bps (some bps) weightBps = 0 := by
  simp [backing]

/-- From someone who counts as anyone does, the backing is the plain difference. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.backing Mos.Forecast.evenWeight]
theorem backing_of_anyone (believedBps baseBps : Nat) :
    backing believedBps (some baseBps) evenWeight = (likelihood believedBps : Int) - (likelihood baseBps : Int) := by
  simp [backing, evenWeight]

/-- With no base rate known, a belief is measured against even odds. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.backing]
theorem backing_without_history (believedBps weightBps : Nat) :
    backing believedBps none weightBps = backing believedBps (some 5000) weightBps := by
  simp [backing]

/-! ## Whether a stretch of time went better than what came before

A report over a stretch sets the average score of the forecasts settled in it against
the average of those settled before. Lower is better. With few forecasts on either
side the difference says little, and is not called either way. -/

inductive Trend where
  | better
  | worse
  | same
  /-- Too few scored forecasts on one side or the other to say. -/
  | untold
  deriving Repr, DecidableEq

/-- How a stretch compares with what came before it, from the average score and the
number scored on each side. It is called only when both sides have as many scored
forecasts as make a forecaster's own record count. -/
def trend (during before : Option Nat) (scoredDuring scoredBefore : Nat) : Trend :=
  match during, before with
  | some d, some b =>
    if scoredDuring < provenAt ∨ scoredBefore < provenAt then .untold
    else if d < b then .better else if b < d then .worse else .same
  | _, _ => .untold

/-- A stretch is not called better or worse on fewer scored forecasts than make a record. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.provenAt Mos.Forecast.trend]
theorem few_forecasts_tell_nothing (during before : Option Nat) (scoredDuring scoredBefore : Nat)
    (h : scoredDuring < provenAt ∨ scoredBefore < provenAt) : trend during before scoredDuring scoredBefore = .untold := by
  cases during <;> cases before <;> simp [trend, h]

/-- When it is called, better means a lower average score, and worse a higher one. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.trend]
theorem better_is_lower (d b scoredDuring scoredBefore : Nat) :
    (trend (some d) (some b) scoredDuring scoredBefore = .better → d < b) ∧
    (trend (some d) (some b) scoredDuring scoredBefore = .worse → b < d) := by
  simp only [trend]
  constructor <;> intro h <;> split at h <;> first | contradiction | (split at h <;> first | assumption | (split at h <;> first | assumption | contradiction) | contradiction)

/-! ## Forecasts that cannot both be right as stated

Two forecasts by one person about the same number at the same moment constrain each
other. If one cannot happen without the other, it cannot be the likelier. If they
cannot both happen, their likelihoods cannot come to more than certainty; if one of
them must happen, not to less. -/

/-- What a forecast claims of a number: that it ends on one side of a level, with how
likely. `up` is above the level, otherwise below; `strict` leaves the level itself out. -/
structure Claim where
  up : Bool
  strict : Bool
  level : Int
  bps : Nat
  deriving Repr, DecidableEq

/-- Whether the first cannot happen without the second. -/
private def Claim.implies (a b : Claim) : Bool :=
  if a.up = b.up then
    if a.level = b.level then a.strict || !b.strict
    else if a.up then decide (b.level < a.level) else decide (a.level < b.level)
  else false

/-- Whether the two cannot both happen. -/
private def Claim.excludes (a b : Claim) : Bool :=
  if a.up = b.up then false
  else
    let (above, below) := if a.up then (a, b) else (b, a)
    decide (below.level < above.level) || (decide (below.level = above.level) && (above.strict || below.strict))

/-- Whether one of the two must happen. -/
private def Claim.covers (a b : Claim) : Bool :=
  if a.up = b.up then false
  else
    let (above, below) := if a.up then (a, b) else (b, a)
    decide (above.level < below.level) || (decide (above.level = below.level) && !(above.strict && below.strict))

/-- How two claims disagree. -/
inductive Clash where
  | agree
  /-- The first cannot happen without the second, and is called the likelier. -/
  | firstTooLikely
  /-- The second cannot happen without the first, and is called the likelier. -/
  | secondTooLikely
  /-- They cannot both happen, and together are called more than certain. -/
  | tooMuch
  /-- One of them must happen, and together they are called less than certain. -/
  | tooLittle
  deriving Repr, DecidableEq

def clash (a b : Claim) : Clash :=
  if a.implies b && decide (likelihood b.bps < likelihood a.bps) then .firstTooLikely
  else if b.implies a && decide (likelihood a.bps < likelihood b.bps) then .secondTooLikely
  else if a.excludes b && decide (10000 < likelihood a.bps + likelihood b.bps) then .tooMuch
  else if a.covers b && decide (likelihood a.bps + likelihood b.bps < 10000) then .tooLittle
  else .agree

/-- A claim does not disagree with itself. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.clash]
theorem clash_with_itself (a : Claim) : clash a a = .agree := by
  simp [clash, Claim.implies, Claim.excludes, Claim.covers]

/-- Two claims that agree, the first of which cannot happen without the second, do not
call the first the likelier. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.clash Mos.Forecast.likelihood]
theorem agreeing_respects_implication (a b : Claim) (h : clash a b = .agree) (hi : a.implies b = true) :
    likelihood a.bps ≤ likelihood b.bps := by
  simp only [clash, hi, Bool.true_and] at h
  by_cases hlt : likelihood b.bps < likelihood a.bps
  · simp [hlt] at h
  · omega

/-- Two claims that agree and cannot both happen are not together called more than certain. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.clash Mos.Forecast.likelihood]
theorem agreeing_exclusive_within_certainty (a b : Claim) (h : clash a b = .agree) (he : a.excludes b = true) :
    likelihood a.bps + likelihood b.bps ≤ 10000 := by
  have hup : a.up ≠ b.up := by
    intro heq
    simp [Claim.excludes, heq] at he
  have hab : a.implies b = false := by simp [Claim.implies, hup]
  have hba : b.implies a = false := by simp [Claim.implies, Ne.symm hup]
  simp only [clash, hab, hba, he, Bool.false_and, Bool.true_and] at h
  by_cases hgt : 10000 < likelihood a.bps + likelihood b.bps
  · simp [hgt] at h
  · omega

/-! ### Claims about different moments

A claim holds at the end of a stretch of time, or at any moment in it. The stretch runs
from when the forecast was stated, since only what becomes known after that counts, to
its deadline. -/

/-- When a claim is about. -/
structure When where
  /-- Whether any moment of the stretch will do, and not only its end. -/
  anyTime : Bool
  since : Int
  till : Int
  deriving Repr, DecidableEq

/-- Whether every moment at which the first could be made true is one the second
counts. The end of a stretch is counted by a claim about that same end, and by a claim
about any moment of a stretch it lies in. Any moment of a stretch is counted only by a
claim about any moment of a stretch that holds the whole of it. -/
private def When.inside (a b : When) : Bool :=
  if b.anyTime then
    if a.anyTime then decide (b.since ≤ a.since) && decide (a.till ≤ b.till)
    else decide (b.since < a.till) && decide (a.till ≤ b.till)
  else !a.anyTime && decide (a.till = b.till)

/-- Whether both are about the end of the same stretch. -/
private def When.together (a b : When) : Bool := !a.anyTime && !b.anyTime && decide (a.till = b.till)

/-- Whether a claim counts a moment: the end of its stretch, or any moment in it after
it was stated. -/
def When.counts (w : When) (moment : Int) : Bool :=
  if w.anyTime then decide (w.since < moment) && decide (moment ≤ w.till) else decide (moment = w.till)

/-- Whether there is a moment both count. Two claims about any moment of a stretch
meet where their stretches overlap; a claim about the end of a stretch meets one that
counts that end. -/
private def When.meet (a b : When) : Bool :=
  if a.anyTime && b.anyTime then
    decide (a.since < a.till) && decide (b.since < b.till) && decide (a.since < b.till) && decide (b.since < a.till)
  else if a.anyTime then b.inside a
  else a.inside b

/-- Claims that meet have a moment both count. At that moment the number stands
somewhere: if every place it can stand makes one of them true, one of them comes true. -/
theorem meet_has_a_moment (a b : When) (h : a.meet b = true) : ∃ moment, a.counts moment = true ∧ b.counts moment = true := by
  unfold When.meet at h
  by_cases ha : a.anyTime = true <;> by_cases hb : b.anyTime = true
  · simp only [ha, hb, Bool.and_self, ite_true, Bool.and_eq_true, decide_eq_true_eq] at h
    by_cases hle : a.till ≤ b.till
    · exact ⟨a.till, by simp [When.counts, ha, hb]; omega⟩
    · exact ⟨b.till, by simp [When.counts, ha, hb]; omega⟩
  · simp only [ha, hb, Bool.and_false, Bool.false_eq_true, ite_false, ite_true, When.inside, Bool.and_eq_true, decide_eq_true_eq] at h
    exact ⟨b.till, by simp [When.counts, ha, hb]; omega⟩
  · simp only [ha, hb, Bool.false_and, Bool.false_eq_true, ite_false, When.inside, ite_true, Bool.and_eq_true, decide_eq_true_eq] at h
    exact ⟨a.till, by simp [When.counts, ha, hb]; omega⟩
  · simp only [ha, hb, Bool.false_and, Bool.false_eq_true, ite_false, When.inside, Bool.not_false, Bool.true_and, decide_eq_true_eq] at h
    exact ⟨a.till, by simp [When.counts, ha, hb, h]⟩

/-- How two claims about a number disagree, each with when it is about. One that
cannot happen without the other, at a moment the other counts, cannot be the likelier.
That two cannot both happen is said only of the same moment: reaching a level at one
moment excludes nothing at another. That one of two must happen is said wherever
there is a moment both count, since at that moment the number stands on the side of one. -/
def clashWhen (a b : Claim) (wa wb : When) : Clash :=
  if a.implies b && wa.inside wb && decide (likelihood b.bps < likelihood a.bps) then .firstTooLikely
  else if b.implies a && wb.inside wa && decide (likelihood a.bps < likelihood b.bps) then .secondTooLikely
  else if wa.together wb && a.excludes b && decide (10000 < likelihood a.bps + likelihood b.bps) then .tooMuch
  else if wa.meet wb && a.covers b && decide (likelihood a.bps + likelihood b.bps < 10000) then .tooLittle
  else .agree

/-- About the end of the same stretch, it is the disagreement of the claims themselves. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.clashWhen Mos.Forecast.clash]
theorem clashWhen_at_one_moment (a b : Claim) (since₁ since₂ till : Int) :
    clashWhen a b ⟨false, since₁, till⟩ ⟨false, since₂, till⟩ = clash a b := by
  simp [clashWhen, clash, When.inside, When.together, When.meet]

/-- Reaching a level at some moment says nothing of where things stand at the end: a
claim about any moment is never held to imply one about the end of a stretch. -/
theorem any_time_implies_no_ending (a b : When) (ha : a.anyTime = true) (hb : b.anyTime = false) : a.inside b = false := by
  simp [When.inside, ha, hb]

/-- Claims about different moments are never said to exclude each other. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.clashWhen]
theorem only_together_excludes (a b : Claim) (wa wb : When) (h : wa.together wb = false) :
    clashWhen a b wa wb ≠ .tooMuch := by
  simp only [clashWhen, h, Bool.false_and]
  (repeat' split) <;> simp_all

/-- Claims with no moment both count are never said to cover everything between them. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.clashWhen]
theorem only_meeting_covers (a b : Claim) (wa wb : When) (h : wa.meet wb = false) :
    clashWhen a b wa wb ≠ .tooLittle := by
  simp only [clashWhen, h, Bool.false_and]
  (repeat' split) <;> simp_all

/-! ### Claims about related numbers

Some numbers are bound to one another whatever they turn out to be. A quarter's revenue
is part of its year's, and no quarter's revenue is below nothing: the year comes to at
least that quarter and whatever the other quarters are already known to be. Gross profit
is never more than revenue. A claim about one such number then cannot come true without
a claim about the other, and cannot be the likelier of the two. -/

/-- Whether a claim is true of a number. -/
def Claim.holdsOf (c : Claim) (v : Int) : Prop :=
  if c.up then (if c.strict then c.level < v else c.level ≤ v)
  else (if c.strict then v < c.level else v ≤ c.level)

/-- A claim that cannot happen without another is true of no number the other is false of. -/
theorem implies_is_sound (a b : Claim) (v : Int) (h : a.implies b = true) (ha : a.holdsOf v) : b.holdsOf v := by
  unfold Claim.implies at h
  unfold Claim.holdsOf at ha ⊢
  cases hau : a.up <;> cases hbu : b.up <;> cases has : a.strict <;> cases hbs : b.strict <;>
    simp_all <;> (try split at h) <;> (try simp_all) <;> omega

/-- How a first number is bound to a second. -/
inductive Link where
  /-- The first is a part of the second, no part of which is below nothing; the parts
  already known, other than the first, come to `others`. The second is at least the
  first and those together. -/
  | part (others : Int)
  /-- The first is the whole and the second a part of it: the reverse. -/
  | whole (others : Int)
  /-- The first is never more than the second. -/
  | under
  /-- The first is never less than the second. -/
  | over
  deriving Repr, DecidableEq

/-- What a link says of the two numbers. -/
def Link.binds : Link → Int → Int → Prop
  | .part others, x, y => x + others ≤ y
  | .whole others, x, y => y + others ≤ x
  | .under, x, y => x ≤ y
  | .over, x, y => y ≤ x

/-- What a claim about the first number says of the second, where it says anything. A
part being above a level puts the whole above that level and the other parts known; the
whole being below a level puts a part below it less the others. That a part is below a
level, or the whole above one, says nothing of the other. -/
def carried (link : Link) (a : Claim) : Option Claim :=
  match link with
  | .part others => if a.up then some { a with level := a.level + others } else none
  | .whole others => if a.up then none else some { a with level := a.level - others }
  | .under => if a.up then some a else none
  | .over => if a.up then none else some a

/-- What is carried over is true of the second number whenever the claim is true of
the first and the numbers are bound as the link says. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.carried]
theorem carried_holds (link : Link) (a c : Claim) (x y : Int) (hb : link.binds x y) (hc : carried link a = some c)
    (ha : a.holdsOf x) : c.holdsOf y := by
  cases link <;> simp only [carried] at hc <;> split at hc <;> simp_all <;> subst hc <;>
    simp only [Link.binds] at hb <;> unfold Claim.holdsOf at ha ⊢ <;> simp_all <;>
    (cases has : a.strict <;> simp_all <;> omega)

/-- Whether a claim about a first number is called likelier than a claim about a second
that it cannot come true without. Said only of two claims about where their numbers
stand at the end of a stretch, whichever ends first: a quarter is reported before its
year, and each claim is judged on its own number as it is then reported. -/
def tooLikelyAcross (link : Link) (a b : Claim) (wa wb : When) : Bool :=
  match carried link a with
  | some c => !wa.anyTime && !wb.anyTime && c.implies b && decide (likelihood b.bps < likelihood a.bps)
  | none => false

/-- It is said only where the first claim cannot be true of its number without the
second being true of its own, and the first is called the likelier. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.tooLikelyAcross Mos.Forecast.likelihood]
theorem too_likely_across_is_sound (link : Link) (a b : Claim) (wa wb : When) (x y : Int)
    (h : tooLikelyAcross link a b wa wb = true) (hb : link.binds x y) :
    (a.holdsOf x → b.holdsOf y) ∧ likelihood b.bps < likelihood a.bps := by
  unfold tooLikelyAcross at h
  split at h
  · rename_i c hc
    simp only [Bool.and_eq_true, decide_eq_true_eq] at h
    exact ⟨fun ha => implies_is_sound c b y h.1.2 (carried_holds link a c x y hb hc ha), h.2⟩
  · contradiction

/-- Nothing is said of a claim about any moment of a stretch: a report that once put a
number on one side says nothing of where the related number ends. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.tooLikelyAcross]
theorem across_only_at_endings (link : Link) (a b : Claim) (wa wb : When) (h : wa.anyTime = true ∨ wb.anyTime = true) :
    tooLikelyAcross link a b wa wb = false := by
  unfold tooLikelyAcross
  split <;> cases h <;> simp_all

/-! ### Several parts and their whole

Two quarters each above a level put the year above the two levels together and whatever
the other quarters are known to be. The year's claim then cannot fail when both
quarters' claims hold. How likely both hold is not given by how likely each is, but it
is at least what the two likelihoods exceed one certainty by: called 90% each, both hold
at least 80% of the time. The year's claim cannot be called less. -/

/-- Whether claims that parts are above their levels put the whole on the side its
claim says: the levels together, and the other parts known, reach the whole's level. -/
def partsCarry (parts : List Claim) (others : Int) (whole : Claim) : Bool :=
  let total := (parts.map (·.level)).sum + others
  parts.all (·.up) && whole.up &&
    (decide (whole.level < total) || (decide (whole.level = total) && (!whole.strict || parts.any (·.strict))))

/-- Each claim holding of the number beside it. -/
def allHold : List Claim → List Int → Prop
  | [], [] => True
  | c :: cs, x :: xs => c.holdsOf x ∧ allHold cs xs
  | _, _ => False

/-- Numbers each above its claim's level come to at least the levels together, and to
more where any of the claims leaves its level out. -/
theorem levels_under_values (parts : List Claim) (xs : List Int) (hup : parts.all (·.up) = true) (h : allHold parts xs) :
    (parts.map (·.level)).sum ≤ xs.sum ∧ (parts.any (·.strict) = true → (parts.map (·.level)).sum < xs.sum) := by
  induction parts generalizing xs with
  | nil => cases xs <;> simp_all [allHold]
  | cons c cs ih =>
    cases xs with
    | nil => simp [allHold] at h
    | cons x rest =>
      simp only [allHold] at h
      simp only [List.all_cons, Bool.and_eq_true] at hup
      have ⟨hle, hlt⟩ := ih rest hup.2 h.2
      have hc := h.1
      unfold Claim.holdsOf at hc
      simp only [hup.1, ite_true] at hc
      simp only [List.map_cons, List.sum_cons, List.any_cons, Bool.or_eq_true]
      constructor
      · split at hc <;> omega
      · intro hs
        cases hcs : c.strict
        · simp [hcs] at hs hc
          have := hlt (List.any_eq_true.mpr hs)
          omega
        · simp [hcs] at hc
          omega

/-- Where the parts carry, the whole's claim holds whenever every part's does and the
whole is at least its parts and the others known. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.partsCarry]
theorem parts_carry_is_sound (parts : List Claim) (others : Int) (whole : Claim) (xs : List Int) (y : Int)
    (hc : partsCarry parts others whole = true) (h : allHold parts xs) (hy : xs.sum + others ≤ y) : whole.holdsOf y := by
  simp only [partsCarry, Bool.and_eq_true, Bool.or_eq_true, decide_eq_true_eq] at hc
  obtain ⟨⟨hup, hw⟩, hlevel⟩ := hc
  have ⟨hle, hlt⟩ := levels_under_values parts xs hup h
  unfold Claim.holdsOf
  simp only [hw, ite_true]
  cases hlevel with
  | inl hbelow => split <;> omega
  | inr hat =>
    obtain ⟨heq, hstrict⟩ := hat
    cases hws : whole.strict
    · simp
      omega
    · simp [hws] at hstrict
      have := hlt (List.any_eq_true.mpr hstrict)
      simp
      omega

/-- The least several things can all hold together, in basis points, whatever they have
to do with one another: what their likelihoods exceed one certainty fewer than their
number by. It can be below nothing, which says nothing. -/
def leastWhenAll (bps : List Nat) : Int :=
  (bps.map (fun b => (likelihood b : Int))).sum - 10000 * ((bps.length : Int) - 1)

/-- Of one thing it is that thing's own likelihood. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.leastWhenAll]
theorem least_of_one (b : Nat) : leastWhenAll [b] = (likelihood b : Int) := by
  simp [leastWhenAll]

/-- It is never more than any one of them is called. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.leastWhenAll]
theorem least_is_under_each (bps : List Nat) (b : Nat) (h : b ∈ bps) : leastWhenAll bps ≤ (likelihood b : Int) := by
  induction bps with
  | nil => cases h
  | cons x rest ih =>
    have hx : (likelihood x : Int) ≤ 10000 := by simp only [likelihood]; omega
    simp only [leastWhenAll, List.map_cons, List.sum_cons, List.length_cons] at *
    cases h with
    | head => 
      have : ((rest.map (fun b => (likelihood b : Int))).sum) ≤ 10000 * (rest.length : Int) := by
        clear ih
        induction rest with
        | nil => simp
        | cons r rs ihr =>
          have hr : (likelihood r : Int) ≤ 10000 := by simp only [likelihood]; omega
          simp only [List.map_cons, List.sum_cons, List.length_cons]
          push_cast
          omega
      push_cast
      omega
    | tail _ hrest =>
      have := ih hrest
      push_cast
      omega

/-- The least a claim about a whole can be called given claims about its parts, where
they carry: none where they do not. -/
def leastFromParts (parts : List Claim) (others : Int) (whole : Claim) : Option Int :=
  if partsCarry parts others whole then some (leastWhenAll (parts.map (·.bps))) else none

/-- Everything a claim leaves out, called as likely as the claim is not. -/
def Claim.opposite (c : Claim) : Claim :=
  { up := !c.up, strict := !c.strict, level := c.level, bps := 10000 - likelihood c.bps }

/-- A claim's opposite holds of exactly the numbers the claim does not. -/
theorem opposite_holds_where_not (c : Claim) (v : Int) : c.opposite.holdsOf v ↔ ¬ c.holdsOf v := by
  unfold Claim.opposite Claim.holdsOf
  cases c.up <;> cases c.strict <;> simp <;> omega

/-! ### Forecasts that something happens

A forecast that something of some kind becomes known in a stretch of time cannot come
true without another that counts everything it counts, over a stretch that holds the
whole of its own. That one thing happening excludes another, or that one of two must
happen, is never said of them. -/

/-- How two forecasts that something happens disagree. `firstWithin` says that whatever
would make the first true is of a kind, and worded in a way, that the second counts;
`secondWithin` the same the other way round. The stretches are from when each was
stated to its deadline. -/
def clashHappening (firstWithin secondWithin : Bool) (firstBps secondBps : Nat) (first second : When) : Clash :=
  let contains (outer inner : When) : Bool := decide (outer.since ≤ inner.since) && decide (inner.till ≤ outer.till)
  if firstWithin && contains second first && decide (likelihood secondBps < likelihood firstBps) then .firstTooLikely
  else if secondWithin && contains first second && decide (likelihood firstBps < likelihood secondBps) then .secondTooLikely
  else .agree

/-- Two forecasts that something happens are never said to exclude each other, or to
cover everything between them. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.clashHappening]
theorem happening_only_implies (firstWithin secondWithin : Bool) (p q : Nat) (a b : When) :
    clashHappening firstWithin secondWithin p q a b ≠ .tooMuch ∧ clashHappening firstWithin secondWithin p q a b ≠ .tooLittle := by
  simp only [clashHappening]
  constructor <;> (repeat' split) <;> simp_all

/-- Called equally likely, they agree whatever they count. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.clashHappening]
theorem happening_as_likely_agrees (firstWithin secondWithin : Bool) (p : Nat) (a b : When) :
    clashHappening firstWithin secondWithin p p a b = .agree := by
  simp [clashHappening]

/-- One that counts nothing the other counts, either way, agrees with it. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.clashHappening]
theorem happening_apart_agrees (p q : Nat) (a b : When) : clashHappening false false p q a b = .agree := by
  simp [clashHappening]

/-- The first is said to be too likely only when it is the likelier, and the second
counts what it counts over a stretch that holds its own. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.clashHappening Mos.Forecast.likelihood]
theorem happening_first_too_likely (firstWithin secondWithin : Bool) (p q : Nat) (a b : When)
    (h : clashHappening firstWithin secondWithin p q a b = .firstTooLikely) :
    firstWithin = true ∧ b.since ≤ a.since ∧ a.till ≤ b.till ∧ likelihood q < likelihood p := by
  simp only [clashHappening] at h
  split at h
  · rename_i hc
    simp only [Bool.and_eq_true, decide_eq_true_eq] at hc
    exact ⟨hc.1.1, hc.1.2.1, hc.1.2.2, hc.2⟩
  · split at h <;> contradiction

/-! ## Reading a likelihood as its forecaster's record says to

Someone who says 80% and is right six times in ten is telling us 60%. What a forecaster
says is read through what became of the other things they called about as likely: of
`scored` such forecasts, `came` came true. A short record moves the reading little; a
long one moves it nearly all the way to how often they were right. -/

/-- What a stated likelihood comes to, given how the forecaster's other forecasts called
about as likely turned out. It is what they said, counted as `countsAs` forecasts that
went exactly as said, taken together with the ones on record. How many that is, is the
installation's to say: none takes the record alone as soon as there is one, and many
make a record slow to be believed. -/
def recalibrated (statedBps came scored countsAs : Nat) : Nat :=
  if scored + countsAs = 0 then likelihood statedBps
  else (min came scored * 10000 + countsAs * likelihood statedBps) / (scored + countsAs)

/-- With no record, a likelihood is read as it was said, whatever what is said counts as. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.recalibrated Mos.Forecast.likelihood]
theorem no_record_reads_as_said (statedBps countsAs : Nat) : recalibrated statedBps 0 0 countsAs = likelihood statedBps := by
  simp only [recalibrated, Nat.min_self, Nat.zero_mul, Nat.zero_add]
  split
  · rfl
  · rename_i h
    exact Nat.mul_div_cancel_left _ (by omega)

/-- However it is read, it is a likelihood. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.recalibrated]
theorem recalibrated_is_a_likelihood (statedBps came scored countsAs : Nat) : recalibrated statedBps came scored countsAs ≤ 10000 := by
  simp only [recalibrated, likelihood]
  have h2 : min statedBps 10000 ≤ 10000 := Nat.min_le_right _ _
  split
  · exact h2
  · apply Nat.div_le_of_le_mul
    have h1 : min came scored ≤ scored := Nat.min_le_right _ _
    generalize min came scored = a at *
    generalize min statedBps 10000 = b at *
    have h3 : countsAs * b ≤ countsAs * 10000 := Nat.mul_le_mul_left _ h2
    omega

/-- A forecaster whose such forecasts all came true is never read as less sure than they said. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.likelihood Mos.Forecast.recalibrated]
theorem always_right_reads_no_lower (statedBps scored countsAs : Nat) :
    likelihood statedBps ≤ recalibrated statedBps scored scored countsAs := by
  simp only [recalibrated, likelihood, Nat.min_self]
  split
  · exact Nat.le_refl _
  · rename_i hpos
    apply (Nat.le_div_iff_mul_le (by omega)).mpr
    have h : min statedBps 10000 ≤ 10000 := Nat.min_le_right _ _
    generalize min statedBps 10000 = b at *
    have h2 : b * scored ≤ 10000 * scored := Nat.mul_le_mul_right _ h
    have h3 : b * countsAs = countsAs * b := Nat.mul_comm _ _
    rw [Nat.mul_add]
    omega

/-- One whose such forecasts never came true is never read as more sure than they said. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.recalibrated Mos.Forecast.likelihood]
theorem never_right_reads_no_higher (statedBps scored countsAs : Nat) :
    recalibrated statedBps 0 scored countsAs ≤ likelihood statedBps := by
  simp only [recalibrated, likelihood, Nat.zero_min, Nat.zero_mul, Nat.zero_add]
  split
  · exact Nat.le_refl _
  · apply Nat.div_le_of_le_mul
    generalize min statedBps 10000 = b
    rw [Nat.add_mul]
    omega

/-- With what is said counting for nothing, a record is taken at its word: of things
called about this likely, the share that came true. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.recalibrated]
theorem counting_for_nothing_reads_the_record (statedBps came scored : Nat) (h : 0 < scored) :
    recalibrated statedBps came scored 0 = min came scored * 10000 / scored := by
  have hne : scored ≠ 0 := by omega
  simp [recalibrated, hne]

/-! ### Choosing what a stated likelihood counts as

Whether reading forecasters through their records helps, and how heavily to do it, can
be asked of the record itself: each way of reading is tried on the forecasts already
decided, each read with what was on record before it was decided, and scored. -/

/-- How many decided forecasts with a record to read them by it takes before one way of
reading is called better than another. -/
def choosesAt : Nat := 4 * provenAt

/-- One way of reading that was tried: what a stated likelihood counted as, and the
average score reading that way earned. -/
structure Tried where
  countsAs : Nat
  meanBrier : Nat
  deriving Repr, DecidableEq

/-- The better of two ways tried: the lower score, and of two that score alike, the one
that takes what was said for more, since it departs least from the forecaster's word. -/
def better (a b : Tried) : Tried :=
  if a.meanBrier < b.meanBrier then a
  else if b.meanBrier < a.meanBrier then b
  else if b.countsAs ≤ a.countsAs then a else b

/-- The way of reading that scored best over `forecasts` forecasts, if there are enough
of them to say. -/
def bestReading (forecasts : Nat) : List Tried → Option Tried
  | [] => none
  | first :: rest => if forecasts < choosesAt then none else some (rest.foldl better first)

@[lungo_claim "lungo.law" subject Mos.Forecast.better]
theorem better_scores_no_worse (a b : Tried) : (better a b).meanBrier ≤ a.meanBrier ∧ (better a b).meanBrier ≤ b.meanBrier := by
  simp only [better]
  constructor <;> (repeat' split) <;> omega

@[lungo_claim "lungo.law" subject Mos.Forecast.better]
theorem best_of_fold_scores_no_worse (rest : List Tried) (first : Tried) :
    (rest.foldl better first).meanBrier ≤ first.meanBrier ∧ ∀ t ∈ rest, (rest.foldl better first).meanBrier ≤ t.meanBrier := by
  induction rest generalizing first with
  | nil => simp
  | cons x xs ih =>
    simp only [List.foldl_cons]
    have step := better_scores_no_worse first x
    have later := ih (better first x)
    constructor
    · exact Nat.le_trans later.1 step.1
    · intro t ht
      cases List.mem_cons.mp ht with
      | inl h =>
        subst h
        exact Nat.le_trans later.1 step.2
      | inr h => exact later.2 t h

/-- No way of reading is chosen on fewer forecasts than it takes to say. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.choosesAt Mos.Forecast.bestReading]
theorem too_few_choose_nothing (forecasts : Nat) (tried : List Tried) (h : forecasts < choosesAt) : bestReading forecasts tried = none := by
  cases tried <;> simp [bestReading, h]

/-- The way chosen scored no worse than any that was tried. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.bestReading]
theorem chosen_scores_no_worse (forecasts : Nat) (tried : List Tried) (best : Tried) (h : bestReading forecasts tried = some best) :
    ∀ t ∈ tried, best.meanBrier ≤ t.meanBrier := by
  cases tried with
  | nil => simp [bestReading] at h
  | cons first rest =>
    simp only [bestReading] at h
    split at h
    · contradiction
    · injection h with h
      subst h
      have all := best_of_fold_scores_no_worse rest first
      intro t ht
      cases List.mem_cons.mp ht with
      | inl h =>
        subst h
        exact all.1
      | inr h => exact all.2 t h

/-! ## The week a regular report covers

A report is made of each whole week once it is over. Days are counted from the first
of January 1970, which was a Thursday; weeks run from Monday. -/

/-- The Monday on or before a day. -/
def weekStart (day : Nat) : Nat := day - (day + 3) % 7

/-- The last whole week before a day: from its Monday up to, and not including, the
Monday of the week the day is in. -/
def lastWeek (today : Nat) : Nat × Nat := (weekStart today - 7, weekStart today)

/-- A week starts on a Monday, on or before the day, and less than a week before it. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.weekStart]
theorem weekStart_is_the_monday (day : Nat) (h : 6 ≤ day) :
    (weekStart day + 3) % 7 = 0 ∧ weekStart day ≤ day ∧ day < weekStart day + 7 := by
  simp only [weekStart]
  omega

/-- The last whole week is seven days long and over by the day it is asked on. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.lastWeek]
theorem lastWeek_is_whole_and_over (today : Nat) (h : 13 ≤ today) :
    (lastWeek today).2 = (lastWeek today).1 + 7 ∧ (lastWeek today).2 ≤ today := by
  simp only [lastWeek, weekStart]
  omega

/-! ## What an action stands to make

An action attached to a forecast gains something if the forecast comes true and loses
something if it does not. Weighed by how likely the forecast is believed to be, and
less what getting in and out costs, that is what it is expected to make. All in basis
points of the amount. -/

/-- What an action is expected to make: its gain if the forecast comes true and its
loss (a negative gain) if it does not, each weighed by the likelihood believed, less
the cost of getting in and of getting out. -/
def expected (believedBps : Nat) (gainBps lossBps : Int) (costBps : Nat) : Int :=
  ((likelihood believedBps : Int) * gainBps + (10000 - (likelihood believedBps : Int)) * lossBps) / 10000 - 2 * (costBps : Int)

/-- Believed certain, it is expected to make its gain less costs; believed impossible, its loss. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.expected]
theorem expected_at_the_extremes (gainBps lossBps : Int) (costBps : Nat) :
    expected 10000 gainBps lossBps costBps = gainBps - 2 * (costBps : Int) ∧
    expected 0 gainBps lossBps costBps = lossBps - 2 * (costBps : Int) := by
  simp [expected, likelihood]

/-- At even odds a gain and a loss of the same size expect nothing but the costs. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.expected]
theorem even_odds_expect_only_costs (sizeBps costBps : Nat) :
    expected 5000 (sizeBps : Int) (-(sizeBps : Int)) costBps = -(2 * (costBps : Int)) := by
  simp only [expected, likelihood]
  have : (min 5000 10000 : Nat) = 5000 := by decide
  rw [this]
  have : ((5000 : Nat) : Int) * (sizeBps : Int) + (10000 - ((5000 : Nat) : Int)) * -(sizeBps : Int) = 0 := by
    simp
    omega
  rw [this]
  simp

/-- How much an action counts for when those on one instrument are taken together:
how far its forecast's backing is from nothing, either way, in basis points, and never
more than the whole of its amount. A belief far below history backs an action as much
as one far above: the action says which way the believer takes it. -/
def conviction (backingBps : Int) : Nat := min backingBps.natAbs 10000

/-- A forecast that only agrees with history adds nothing to what is said about an instrument. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.conviction]
theorem no_backing_no_conviction : conviction 0 = 0 := by decide

/-- Conviction does not depend on which way the belief departs from history, and is never more than the whole. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.conviction]
theorem conviction_is_symmetric_and_bounded (backingBps : Int) :
    conviction (-backingBps) = conviction backingBps ∧ conviction backingBps ≤ 10000 := by
  simp only [conviction, Int.natAbs_neg]
  exact ⟨trivial, Nat.min_le_right _ _⟩

/-- Which of two actions is listed first when they are ordered by what they are
expected to make: one whose stake can be said before one whose cannot, the more
expected first, and otherwise the better backed. -/
def expectedFirst (a b : Option Int × Int) : Bool :=
  match a.1, b.1 with
  | some x, some y => if x = y then b.2.natAbs < a.2.natAbs else y < x
  | some _, none => true
  | none, some _ => false
  | none, none => b.2.natAbs < a.2.natAbs

/-- An action whose stake can be said is never listed after one whose cannot. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.expectedFirst]
theorem said_before_unsaid (x backing other : Int) : expectedFirst (some x, backing) (none, other) = true ∧ expectedFirst (none, other) (some x, backing) = false := by
  simp [expectedFirst]

/-- Nothing is listed before itself. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.expectedFirst]
theorem expectedFirst_irreflexive (a : Option Int × Int) : expectedFirst a a = false := by
  rcases a with ⟨stake, backing⟩
  cases stake <;> simp [expectedFirst]

/-! ## Getting out of what a forecast said to do

An action attached to a forecast is followed on paper. It may say when to get out
before the forecast settles: at a loss of so much, at a gain of so much, or after so
many days. Each time something new is known of the price, this decides whether one of
those has come. A loss is looked at first: on the day both a stop and a time limit
come, the stop is the reason. -/

inductive Leave where
  | stay
  | stop
  | target
  | time
  deriving Repr, DecidableEq

/--
Whether to get out, from the return so far in basis points (after what getting in
cost), how long it has been held, and the rules given: a stop and a target as
distances in basis points, and a number of days.
-/
def leave (returnBps : Int) (stopBps targetBps : Option Nat) (heldDays : Nat) (maxDays : Option Nat) : Leave :=
  if stopBps.any (fun stop => returnBps ≤ -(stop : Int)) then .stop
  else if targetBps.any (fun target => (target : Int) ≤ returnBps) then .target
  else if maxDays.any (fun days => days ≤ heldDays) then .time
  else .stay

/-- With no rule given, an action stays until its forecast settles. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.leave]
theorem no_rule_stays (returnBps : Int) (heldDays : Nat) : leave returnBps none none heldDays none = .stay := by
  simp [leave]

/-- A loss as large as the stop gets out as a stop, whatever else has come. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.leave]
theorem stop_comes_first (returnBps : Int) (stop : Nat) (targetBps : Option Nat) (heldDays : Nat) (maxDays : Option Nat)
    (h : returnBps ≤ -(stop : Int)) : leave returnBps (some stop) targetBps heldDays maxDays = .stop := by
  simp [leave, h]

/-- Between the stop and the target, and within its days, it stays. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.leave]
theorem within_bounds_stays (returnBps : Int) (stop target heldDays days : Nat)
    (lower : -(stop : Int) < returnBps) (upper : returnBps < (target : Int)) (young : heldDays < days) :
    leave returnBps (some stop) (some target) heldDays (some days) = .stay := by
  have a : ¬ returnBps ≤ -(stop : Int) := by omega
  have b : ¬ (target : Int) ≤ returnBps := by omega
  have c : ¬ days ≤ heldDays := by omega
  simp [leave, a, b, c]

/-! ## What a record says of how sure its forecaster is

Of the forecasts someone called likely, say, some share came true. If they were as
sure as they should be, that share is about what they said on average. How far off it
may be by chance depends on how many forecasts there are and on what was said: the
number that come true varies about the sum of the likelihoods with a spread that is the
sum, over the forecasts, of each likelihood times what is left of certainty. A record
is said to lean one way only when it is further off than twice that spread allows, and
has as many forecasts as make a record count. -/

/-- One settled forecast: what was said, and whether it came true. -/
structure Call where
  statedBps : Nat
  cameTrue : Bool
  deriving Repr, DecidableEq

/-- How what was said compares with what happened. -/
inductive Sureness where
  /-- Too few forecasts to say. -/
  | untold
  /-- What was said is about what happened. -/
  | about
  /-- Things were called likelier than they turned out. -/
  | tooHigh
  /-- Things were called less likely than they turned out. -/
  | tooLow
  deriving Repr, DecidableEq

/-- From the numbers: how many forecasts, how many came true, the sum of what was said
and the sum of each likelihood times what it leaves of certainty. -/
def surenessOf (forecasts came statedSum spread : Nat) : Sureness :=
  if forecasts < provenAt then .untold
  else if came * 10000 < statedSum ∧ 4 * spread < (statedSum - came * 10000) * (statedSum - came * 10000) then .tooHigh
  else if statedSum < came * 10000 ∧ 4 * spread < (came * 10000 - statedSum) * (came * 10000 - statedSum) then .tooLow
  else .about

private def cameTrue (calls : List Call) : Nat := calls.foldl (fun n c => if c.cameTrue then n + 1 else n) 0

private def statedSum (calls : List Call) : Nat := calls.foldl (fun sum c => sum + likelihood c.statedBps) 0

private def spread (calls : List Call) : Nat := calls.foldl (fun sum c => sum + likelihood c.statedBps * (10000 - likelihood c.statedBps)) 0

/-- What a set of settled forecasts says of how sure their forecaster is. -/
def sureness (calls : List Call) : Sureness :=
  surenessOf calls.length (cameTrue calls) (statedSum calls) (spread calls)

/-- Nothing is said of fewer forecasts than make a record. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.provenAt Mos.Forecast.surenessOf]
theorem few_calls_tell_nothing (forecasts came statedSum spread : Nat) (h : forecasts < provenAt) :
    surenessOf forecasts came statedSum spread = .untold := by
  simp [surenessOf, h]

/-- Said to be too high, less did happen than was said. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.surenessOf]
theorem too_high_is_high (forecasts came statedSum spread : Nat) (h : surenessOf forecasts came statedSum spread = .tooHigh) :
    came * 10000 < statedSum := by
  simp only [surenessOf] at h
  split at h
  · contradiction
  · split at h
    · rename_i hc
      exact hc.1
    · split at h <;> contradiction

/-- Said to be too low, more did happen than was said. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.surenessOf]
theorem too_low_is_low (forecasts came statedSum spread : Nat) (h : surenessOf forecasts came statedSum spread = .tooLow) :
    statedSum < came * 10000 := by
  simp only [surenessOf] at h
  split at h
  · contradiction
  · split at h
    · contradiction
    · split at h
      · rename_i hc
        exact hc.1
      · contradiction

/-- A record on which exactly as much happened as was said leans neither way. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.provenAt Mos.Forecast.surenessOf]
theorem as_said_leans_neither_way (forecasts came spread : Nat) (h : provenAt ≤ forecasts) :
    surenessOf forecasts came (came * 10000) spread = .about := by
  have hn : ¬ forecasts < provenAt := by omega
  simp [surenessOf, hn]

/-! ## The period before

A figure can be forecast against the one reported for the period before it. Periods are
numbered from 1 within their year, `perYear` of them in a year (4 quarters, 12 months, 1
for a year itself). -/

def previousPeriod (year : Int) (index perYear : Nat) : Int × Nat :=
  if 1 < index then (year, index - 1) else (year - 1, perYear)

def nextPeriod (year : Int) (index perYear : Nat) : Int × Nat :=
  if index < perYear then (year, index + 1) else (year + 1, 1)

/-- The period after the one before is the period itself. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.nextPeriod Mos.Forecast.previousPeriod]
theorem next_of_previous (year : Int) (index perYear : Nat) (h1 : 1 ≤ index) (h2 : index ≤ perYear) :
    nextPeriod (previousPeriod year index perYear).1 (previousPeriod year index perYear).2 perYear = (year, index) := by
  unfold previousPeriod nextPeriod
  by_cases h : 1 < index
  · simp only [h, ite_true]
    have : index - 1 < perYear := by omega
    simp only [this, ite_true]
    congr 1
    omega
  · simp only [h, ite_false]
    have hi : index = 1 := by omega
    simp [hi]

/-- The period before is still a period of its year: numbered from 1 to `perYear`. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.previousPeriod]
theorem previous_is_a_period (year : Int) (index perYear : Nat) (h1 : 1 ≤ index) (h2 : index ≤ perYear) :
    1 ≤ (previousPeriod year index perYear).2 ∧ (previousPeriod year index perYear).2 ≤ perYear := by
  unfold previousPeriod
  split <;> simp <;> omega

/-! ## The same period a year before

A seasonal business's figure is compared with the same period of the year before: a
year's third quarter with the third quarter of the year before. -/

def yearBefore (year : Int) (index : Nat) : Int × Nat := (year - 1, index)

/-- So many periods back, one at a time. -/
def back (perYear : Nat) : Nat → Int × Nat → Int × Nat
  | 0, p => p
  | k + 1, p => back perYear k (previousPeriod p.1 p.2 perYear)

@[lungo_claim "lungo.equals" subject Mos.Forecast.back]
theorem back_add (perYear m n : Nat) (p : Int × Nat) : back perYear (m + n) p = back perYear n (back perYear m p) := by
  induction m generalizing p with
  | zero => simp [back]
  | succ m ih =>
    rw [Nat.add_right_comm, back, back, ih]

@[lungo_claim "lungo.law" subject Mos.Forecast.back]
theorem back_within_the_year (perYear : Nat) : ∀ (k : Nat) (year : Int) (index : Nat), k < index →
    back perYear k (year, index) = (year, index - k) := by
  intro k
  induction k with
  | zero => intro year index _; simp [back]
  | succ k ih =>
    intro year index h
    have hstep : previousPeriod year index perYear = (year, index - 1) := by
      simp [previousPeriod]
      omega
    simp only [back, hstep]
    rw [ih year (index - 1) (by omega)]
    congr 1
    omega

/-- The same period a year before is as many periods back as a year has. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.back Mos.Forecast.yearBefore]
theorem year_before_is_a_year_of_periods_back (year : Int) (index perYear : Nat) (h1 : 1 ≤ index) (h2 : index ≤ perYear) :
    back perYear perYear (year, index) = yearBefore year index := by
  have split : back perYear perYear (year, index) = back perYear ((index - 1) + 1 + (perYear - index)) (year, index) := by
    have e : (index - 1) + 1 + (perYear - index) = perYear := by omega
    rw [e]
  have to_last : back perYear (index - 1) (year, index) = (year, 1) := by
    rw [back_within_the_year perYear _ year index (by omega)]
    congr 1
    omega
  have over : back perYear (index - 1 + 1) (year, index) = (year - 1, perYear) := by
    rw [back_add, to_last]
    simp [back, previousPeriod]
  rw [split, back_add, over, back_within_the_year perYear _ (year - 1) perYear (by omega)]
  simp [yearBefore]
  omega

/-! ## The next period not yet reported

A question asked again and again of a reported figure asks, each time, of the period
after the latest one reported by then. -/

/-- Whether period `b` comes after period `a`. -/
def later (a b : Int × Nat) : Bool := decide (a.1 < b.1) || (decide (a.1 = b.1) && decide (a.2 < b.2))

/-- The later of two periods. -/
def laterOf (a b : Int × Nat) : Int × Nat := if later a b then b else a

/-- The latest of the periods reported; none when none is. -/
private def latestPeriod : List (Int × Nat) → Option (Int × Nat)
  | [] => none
  | p :: ps => some (ps.foldl laterOf p)

/-- Which period an asking is of: the one after the latest reported. -/
def askedOf (reported : List (Int × Nat)) (perYear : Nat) : Option (Int × Nat) :=
  (latestPeriod reported).map (fun p => nextPeriod p.1 p.2 perYear)

/-- Whether `a` is no later than `b`. -/
def noLater (a b : Int × Nat) : Prop := a.1 < b.1 ∨ (a.1 = b.1 ∧ a.2 ≤ b.2)

@[lungo_claim "lungo.law" subject Mos.Forecast.laterOf]
theorem noLater_laterOf_left (a b : Int × Nat) : noLater a (laterOf a b) := by
  unfold laterOf later noLater
  by_cases h1 : a.1 < b.1
  · simp [h1]
  · by_cases h2 : a.1 = b.1 ∧ a.2 < b.2
    · simp [h2]; omega
    · simp only [h1, decide_false, Bool.false_or]
      have : (decide (a.1 = b.1) && decide (a.2 < b.2)) = false := by
        simp only [Bool.and_eq_false_iff, decide_eq_false_iff_not]; omega
      simp [this]

@[lungo_claim "lungo.law" subject Mos.Forecast.laterOf]
theorem noLater_laterOf_right (a b : Int × Nat) : noLater b (laterOf a b) := by
  unfold laterOf later noLater
  by_cases h1 : a.1 < b.1
  · simp [h1]
  · by_cases h2 : a.1 = b.1 ∧ a.2 < b.2
    · simp [h2]
    · simp only [h1, decide_false, Bool.false_or]
      have : (decide (a.1 = b.1) && decide (a.2 < b.2)) = false := by
        simp only [Bool.and_eq_false_iff, decide_eq_false_iff_not]; omega
      simp [this]
      omega

theorem noLater_trans (a b c : Int × Nat) (h1 : noLater a b) (h2 : noLater b c) : noLater a c := by
  unfold noLater at *
  omega

@[lungo_claim "lungo.law" subject Mos.Forecast.laterOf]
theorem latest_is_latest (ps : List (Int × Nat)) (p : Int × Nat) : ∀ q ∈ p :: ps, noLater q (ps.foldl laterOf p) := by
  induction ps generalizing p with
  | nil => intro q hq; simp at hq; subst hq; simp only [List.foldl_nil]; unfold noLater; omega
  | cons r rs ih =>
    intro q hq
    simp only [List.foldl_cons]
    rcases List.mem_cons.mp hq with h | h
    · subst h
      exact noLater_trans _ _ _ (noLater_laterOf_left q r) (ih (laterOf q r) (laterOf q r) List.mem_cons_self)
    · rcases List.mem_cons.mp h with h | h
      · subst h
        exact noLater_trans _ _ _ (noLater_laterOf_right p q) (ih (laterOf p q) (laterOf p q) List.mem_cons_self)
      · exact ih (laterOf p r) q (List.mem_cons_of_mem _ h)

/-- The next period comes after the period. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.later Mos.Forecast.nextPeriod]
theorem next_is_later (year : Int) (index perYear : Nat) : later (year, index) (nextPeriod year index perYear) = true := by
  unfold nextPeriod later
  split <;> simp <;> omega

/-- An asking is never of a period already reported: it comes after every one. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.askedOf Mos.Forecast.later]
theorem asked_is_not_reported (reported : List (Int × Nat)) (perYear : Nat) (next : Int × Nat)
    (h : askedOf reported perYear = some next) : ∀ q ∈ reported, later q next = true := by
  intro q hq
  cases reported with
  | nil => simp at hq
  | cons p ps =>
    simp only [askedOf, latestPeriod, Option.map_some, Option.some.injEq] at h
    subst h
    have hl := latest_is_latest ps p q hq
    have hn := next_is_later (ps.foldl laterOf p).1 (ps.foldl laterOf p).2 perYear
    unfold noLater at hl
    unfold later at hn ⊢
    simp only [Bool.or_eq_true, Bool.and_eq_true, decide_eq_true_eq] at hn ⊢
    omega

/-- Whether two groups of scored forecasts score apart by more than chance would give:
each given by how many it holds, the sum of their scores and the sum of their scores'
squares. Their averages must differ by more than twice the standard error of that
difference, `(m₁ - m₂)² > 4 (v₁/n₁ + v₂/n₂)` with each `v` a group's sample variance, here
multiplied through so that it is worked in whole numbers. Fewer than make a record on
either side are never apart. -/
def apartBeyondChance (n₁ s₁ q₁ n₂ s₂ q₂ : Nat) : Bool :=
  if n₁ < provenAt ∨ n₂ < provenAt then false
  else
    let d : Int := (s₁ : Int) * n₂ - (s₂ : Int) * n₁
    let v₁ : Int := (n₂ : Int) * n₂ * ((n₂ : Int) - 1) * ((n₁ : Int) * q₁ - (s₁ : Int) * s₁)
    let v₂ : Int := (n₁ : Int) * n₁ * ((n₁ : Int) - 1) * ((n₂ : Int) * q₂ - (s₂ : Int) * s₂)
    decide (4 * (v₁ + v₂) < d * d * ((n₁ : Int) - 1) * ((n₂ : Int) - 1))

/-- Too few on either side are never told apart. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.apartBeyondChance Mos.Forecast.provenAt]
theorem few_are_never_apart (n₁ s₁ q₁ n₂ s₂ q₂ : Nat) (h : n₁ < provenAt ∨ n₂ < provenAt) :
    apartBeyondChance n₁ s₁ q₁ n₂ s₂ q₂ = false := by
  simp [apartBeyondChance, h]

/-- Which group is named first makes no difference. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.apartBeyondChance]
theorem apart_either_way (n₁ s₁ q₁ n₂ s₂ q₂ : Nat) :
    apartBeyondChance n₁ s₁ q₁ n₂ s₂ q₂ = apartBeyondChance n₂ s₂ q₂ n₁ s₁ q₁ := by
  unfold apartBeyondChance
  by_cases h : n₁ < provenAt ∨ n₂ < provenAt
  · have h' : n₂ < provenAt ∨ n₁ < provenAt := h.symm
    simp [h, h']
  · have h' : ¬ (n₂ < provenAt ∨ n₁ < provenAt) := fun x => h x.symm
    simp only [h, h', if_false]
    congr 1
    apply propext
    constructor <;> intro k <;> grind

/-- Two groups scoring the same, forecast for forecast, are not apart. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.apartBeyondChance]
theorem the_same_are_not_apart : apartBeyondChance 5 10000 20000000 5 10000 20000000 = false := by
  decide

/-- Whether pairs of scores of the same forecasts (what was scored beside what something
else would have scored) differ by more than chance would give: given how many pairs there
are, the sum of their differences and the sum of the differences' squares. The average
difference must be more than twice its standard error, `d̄² > 4 v/n` with `v` the
differences' sample variance, here multiplied through so that it is worked in whole
numbers: `D² (n - 1) > 4 (n Q - D²)`. Fewer pairs than make a record are never apart. -/
def pairedBeyondChance (n : Nat) (d : Int) (q : Nat) : Bool :=
  if n < provenAt then false
  else decide (4 * (((n * q : Nat) : Int) - d * d) < d * d * ((n : Int) - 1))

/-- Too few pairs are never told apart. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.pairedBeyondChance Mos.Forecast.provenAt]
theorem few_pairs_are_never_apart (n : Nat) (d : Int) (q : Nat) (h : n < provenAt) :
    pairedBeyondChance n d q = false := by
  simp [pairedBeyondChance, h]

/-- Which of each pair is taken from which makes no difference. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.pairedBeyondChance]
theorem paired_either_way (n : Nat) (d : Int) (q : Nat) :
    pairedBeyondChance n (-d) q = pairedBeyondChance n d q := by
  simp [pairedBeyondChance, Int.neg_mul_neg]

/-- Pairs that never differ are not apart. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.pairedBeyondChance]
theorem no_difference_is_not_apart (n q : Nat) : pairedBeyondChance n 0 q = false := by
  unfold pairedBeyondChance
  split
  · rfl
  · simp only [Int.mul_zero, Int.zero_mul, Int.sub_zero, decide_eq_false_iff_not]
    omega

/-- Whether a likelihood may be stated again for an open forecast: changing one's mind
states another likelihood than the one standing; saying it still stands, having looked
again, states the same one. -/
def mayStateAgain (standing said : Nat) (stillStands : Bool) : Bool :=
  if stillStands then standing == said else standing != said

/-- A change of mind to the likelihood already standing is no change. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.mayStateAgain]
theorem no_change_is_not_a_change_of_mind (standing : Nat) : mayStateAgain standing standing false = false := by
  simp [mayStateAgain]

/-- Saying a forecast still stands states the likelihood standing. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.mayStateAgain]
theorem still_standing_is_the_same (standing : Nat) : mayStateAgain standing standing true = true := by
  simp [mayStateAgain]

/-- Any likelihood may be stated again in exactly one of the two ways. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.mayStateAgain]
theorem one_way_or_the_other (standing said : Nat) : mayStateAgain standing said true = !mayStateAgain standing said false := by
  unfold mayStateAgain
  cases h : (standing == said) <;> simp_all [bne]

/-! ## Each kind read back from its number

The generated code passes a kind by its number; read back, it is the kind it was. -/

@[lungo_claim "lungo.roundtrip" subject Mos.Forecast.Clash.ofNat]
theorem Clash.ofNat_reads_back (c : Clash) : Clash.ofNat c.ctorIdx = c := Clash.ofNat_ctorIdx c

@[lungo_claim "lungo.roundtrip" subject Mos.Forecast.Hearing.ofNat]
theorem Hearing.ofNat_reads_back (h : Hearing) : Hearing.ofNat h.ctorIdx = h := Hearing.ofNat_ctorIdx h

@[lungo_claim "lungo.roundtrip" subject Mos.Forecast.Leave.ofNat]
theorem Leave.ofNat_reads_back (l : Leave) : Leave.ofNat l.ctorIdx = l := Leave.ofNat_ctorIdx l

@[lungo_claim "lungo.roundtrip" subject Mos.Forecast.Outcome.ofNat]
theorem Outcome.ofNat_reads_back (o : Outcome) : Outcome.ofNat o.ctorIdx = o := Outcome.ofNat_ctorIdx o

@[lungo_claim "lungo.roundtrip" subject Mos.Forecast.Sureness.ofNat]
theorem Sureness.ofNat_reads_back (s : Sureness) : Sureness.ofNat s.ctorIdx = s := Sureness.ofNat_ctorIdx s

@[lungo_claim "lungo.roundtrip" subject Mos.Forecast.Trend.ofNat]
theorem Trend.ofNat_reads_back (t : Trend) : Trend.ofNat t.ctorIdx = t := Trend.ofNat_ctorIdx t

@[lungo_claim "lungo.roundtrip" subject Mos.Forecast.Window.ofNat]
theorem Window.ofNat_reads_back (w : Window) : Window.ofNat w.ctorIdx = w := Window.ofNat_ctorIdx w

/-! ## Small rules the runtime reads -/

/-- The opposite of a claim says the other side, at the rest of the likelihood. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.Claim.opposite]
theorem opposite_says_the_rest (c : Claim) : c.opposite.up = !c.up ∧ c.opposite.level = c.level ∧ c.opposite.bps = 10000 - likelihood c.bps := ⟨rfl, rfl, rfl⟩

/-- A moment counts towards a forecast for its deadline only at the deadline. -/
@[lungo_claim "lungo.decides" subject Mos.Forecast.When.counts]
theorem counts_at_the_deadline (w : When) (h : w.anyTime = false) (moment : Int) : w.counts moment = true ↔ moment = w.till := by
  simp [When.counts, h]

/-- For a forecast of any time before its deadline, every moment after it was stated up to the deadline counts. -/
@[lungo_claim "lungo.decides" subject Mos.Forecast.When.counts]
theorem counts_any_time (w : When) (h : w.anyTime = true) (moment : Int) : w.counts moment = true ↔ w.since < moment ∧ moment ≤ w.till := by
  simp [When.counts, h]

/-- What the parts leave of the whole is said only when the parts carry it. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.leastFromParts]
theorem least_from_parts_only_when_carried (parts : List Claim) (others : Int) (whole : Claim) :
    leastFromParts parts others whole = (if partsCarry parts others whole then some (leastWhenAll (parts.map (·.bps))) else none) := rfl

/-- No scores have no mean. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.mean]
theorem mean_of_nothing : mean [] = none := rfl

/-- The mean of one score is that score. -/
@[lungo_claim "lungo.equals" subject Mos.Forecast.mean]
theorem mean_of_one (s : Nat) : mean [s] = some s := by
  simp [mean]

@[lungo_claim "lungo.decides" subject Mos.Forecast.proven]
theorem proven_iff (scored : Nat) : proven scored = true ↔ provenAt ≤ scored := by
  simp [proven]

@[lungo_claim "lungo.decides" subject Mos.Forecast.telling]
theorem telling_iff (windows : Nat) : telling windows = true ↔ tellingAt ≤ windows := by
  simp [telling]

/-- Worse than even odds is a mean score above what saying 50% earns. -/
@[lungo_claim "lungo.decides" subject Mos.Forecast.worse]
theorem worse_iff (meanBrier : Nat) : worse meanBrier = true ↔ brier 5000 true < meanBrier := by
  rw [show brier 5000 true = 2500 by decide]
  simp [worse]

/-- Calls said to be too sure came true less often than they were called. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.sureness]
theorem sureness_too_high (calls : List Call) (h : sureness calls = .tooHigh) : cameTrue calls * 10000 < statedSum calls :=
  too_high_is_high _ _ _ _ h

/-- Calls said to be too unsure came true more often than they were called. -/
@[lungo_claim "lungo.law" subject Mos.Forecast.sureness]
theorem sureness_too_low (calls : List Call) (h : sureness calls = .tooLow) : statedSum calls < cameTrue calls * 10000 :=
  too_low_is_low _ _ _ _ h

end Mos.Forecast
