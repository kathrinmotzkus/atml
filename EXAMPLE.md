# Worked Example — Vehicle Rental (design blueprint)

This document records the **concept** for a worked ATML example: a vehicle
rental. It captures the structure and every design decision we have agreed on,
so the actual ATML and plain-TOML versions can be written from it later. It is
a blueprint, not yet the example itself.

All identifiers are in English. Where a value depends on a local legal frame,
that frame is the **European Union** (see §2).

---

## 1. Purpose

The example exists to do three things at once:

1. **Make ATML's benefit visible.** The same data is written twice — once in
   ATML (DRY, structured through inheritance and enums) and once in plain TOML
   (with all repetition spelled out). Seeing the repetition appear in TOML and
   vanish in ATML is more convincing than any prose.
2. **Stress-test the language.** A larger, realistic model forces the features
   to cooperate and exposes what isolated snippets hide.
3. **Serve as a flattener test vector.** The ATML input plus its expected TOML
   output is exactly an "input → expected output" pair for the future flattener,
   and a conformance oracle for the Rust `toml_dom` implementation.

A vehicle rental was chosen because the redundancy in it is **real**, not
constructed, and because everyone can follow the domain.

---

## 2. The domain frame and its root

The root concept is a **rentable vehicle**. It plays two roles at once:

- a **conceptual frame** — it defines what the document is about. Its criterion
  is "has wheels", which is why an elephant, an oven, or a portion of ice cream
  are not vehicles here.
- a **technical base template** — the table every vehicle inherits from
  (`[vehicle]`).

**Locale frame — European Union.** Wherever a value depends on a legal or
regulatory frame, this example is set in the EU, because those values are
harmonized EU-wide. This applies to driving-licence categories (`B`, `C1`,
`CE`, …), the weight thresholds tied to them, the currency (`EUR`), and the
environmental / emission classes. The example states this explicitly so the
values are unambiguous.

Note the honest limit that runs through the whole project: "has wheels" is a
rule about *meaning*. Neither TOML nor ATML can enforce it; nothing stops
someone writing an entry without `wheels`. Enforcing the domain criterion is
the job of the consuming application (or a validator given the rule), never of
the format. The grammar draws shapes; meaning lives outside it.

---

## 3. Core structure: two crossing axes

A concrete vehicle sits at the intersection of two independent axes and
inherits from both (multiple inheritance, first-wins):

- **Class axis (build)** — a hierarchy: *vehicle type → subclass*.
- **Drive axis** — vehicle-independent drive types.

In plain TOML this crossing is exactly what produces noise: every electric
vehicle must repeat the electric-drive data, every compact car must repeat the
compact-class data. ATML factors both out through inheritance.

---

## 4. Layers, and what each one carries

Attributes are placed by their **origin**. Three origins, three homes, plus the
intersection:

- **Base — `[vehicle]`** (rental-intrinsic): `currency = "EUR"`,
  `min_rental_period = 1d`, `status = Status::available` (default, per-vehicle
  overridable), and the `location` field (value set per vehicle).
- **Class (build-intrinsic):** `wheels`, `seats`, `licence_class` (a
  `LicenceClass` enum, omitted where no licence is required), `cargo_volume`,
  plus `deposit` and `base_rate` (both scale with the vehicle, so they live on
  the class, not the base).
- **Drive (drive-intrinsic, vehicle-independent):** `energy_source`,
  `environmental_class` (enum), `local_emissions` (enum), `rate_modifier`
  (a factor), and `energy_cost` (the carrier's unit price, a rate quantity;
  omitted for `Muscle`, which has no carrier). Absolute emissions (g/km) are
  vehicle-specific and are *not* here.
- **Concrete vehicle (identity + cross-axis values):** `id`, `location`, and the
  values that depend on *both* class and drive — `range`, `tank_capacity`,
  `charge_time`. The daily rate is **not stored**: the app computes it as
  `base_rate` (from the class) × `rate_modifier` (from the drive).

The reason range / tank / charge-time live on the concrete vehicle: they depend
on the **combination** of class and drive (a compact Diesel and a compact
Electric have different ranges), so they belong where the two axes meet, not on
either axis alone. Keeping the drive templates vehicle-independent is also what
lets a car-Electric and a bike-Electric share the same `[drive.electric]`
template instead of duplicating it.

---

## 5. The class hierarchy (subclasses)

Three vehicle types, ten subclasses in total — broad enough to create real
repetition in TOML, small enough to stay readable.

- **truck** — by permissible gross weight, which also fixes the EU driving-
  licence category:
  - `light` (≤ 3.5 t, licence class B)
  - `medium` (7.5 t, licence class C1)
  - `heavy` (40 t, licence class CE)
- **car** — by size:
  - `small`
  - `compact`
  - `estate`
  - `van`
- **twowheeler** — the three-way split:
  - `motorized` (motorcycle / scooter)
  - `ebike` (pedelec)
  - `bicycle` (non-motorized)

The two-wheeler branch is itself multi-level (`twowheeler → ebike → concrete`),
which is what exercises ATML's transitive inheritance. The licence classes and
weight thresholds above are **EU driving-licence categories**, identical across
the European Union.

Agreed build attributes per subclass (illustrative values; `cargo_volume` in
`m^3`, money in `EUR`; `licence_class` references the `LicenceClass` enum and is
**omitted** where no licence is required):

```
class                 wheels  seats  licence_class  cargo_volume  deposit   base_rate
truck.light             4       3    B                 20m^3       500EUR     89EUR
truck.medium            4       3    C1                40m^3      1000EUR    149EUR
truck.heavy             6       2    CE                90m^3      2500EUR    299EUR
car.small               4       4    B                0.3m^3       200EUR     39EUR
car.compact             4       5    B                0.5m^3       250EUR     49EUR
car.estate              4       5    B                1.5m^3       300EUR     59EUR
car.van                 4       7    B                  4m^3       400EUR     79EUR
twowheeler.motorized    2       2    A                  0m^3       300EUR     45EUR
twowheeler.ebike        2       1    (omitted)          0m^3       150EUR     25EUR
twowheeler.bicycle      2       1    (omitted)          0m^3        80EUR     15EUR
```

The DRY win is concrete here: values shared across a whole vehicle type live on
the **type template**, not in each subclass. All cars share `wheels = 4` and
`licence_class = B` (declared once on `[class.car]`); all two-wheelers share
`wheels = 2` (on `[class.twowheeler]`). Plain TOML would repeat these in every
subclass; inheritance removes the repetition.

### The concrete fleet

Concrete vehicles are **array-of-tables** elements that inherit from one class
and one drive (multiple inheritance, first-wins), e.g.
`[[vehicle : class.car.compact, drive.electric]]`. Each carries `id`,
`location`, and the cross-axis values that fit its drive; fields that do not
apply to a drive are simply omitted. Illustrative fleet:

```
inherits (class, drive)               id       location   range    tank_capacity   charge_time
car.compact        + drive.electric   EV-001   Berlin     350km    (omitted)       30min
car.compact        + drive.diesel     DC-002   Hamburg    900km    50L             (omitted)
truck.light        + drive.petrol     TP-003   Munich     650km    80L             (omitted)
car.van            + drive.hydrogen   HV-004   Cologne    500km    6kg             (omitted)
twowheeler.ebike   + drive.electric   EB-005   Berlin      60km    (omitted)       4h
twowheeler.bicycle + drive.muscle     BC-006   Hamburg    (omitted) (omitted)      (omitted)
```

The drive-dependent pattern is visible: electric vehicles carry `range` +
`charge_time`, combustion vehicles `range` + `tank_capacity`, and the bicycle
none of them (it only has `id` and `location`). `tank_capacity` is a volume for
liquids (`50L`) and a mass for gases (`6kg`) — both valid quantities. Values
for the same class differ by drive (electric compact 350km vs diesel compact
900km), which is exactly why they live on the vehicle, not on either axis.

---

## 6. The drive axis

Drive types (the `DriveType` enum values):
`Petrol`, `Diesel`, `Electric`, `NaturalGas`, `Hydrogen`, `Muscle`.

Agreed drive-template values (`environmental_class` and `local_emissions` are
themselves enums; `energy_cost` is the carrier's unit price, now expressible as
a rate quantity):

```
drive         energy_source   environmental_class   local_emissions   rate_modifier   energy_cost
Petrol        "petrol"        D                     combustion        1.00            1.80EUR/L
Diesel        "diesel"        D                     combustion        1.00            1.70EUR/L
Electric      "electricity"   A                     zero              1.15            0.40EUR/kWh
NaturalGas    "natural_gas"   C                     combustion        0.95            1.20EUR/kg
Hydrogen      "hydrogen"      B                     zero              1.20            12.00EUR/kg
Muscle        "human"         A                     zero              0.50            (none)
```

`Muscle` omits `energy_cost` on purpose: it has no energy carrier, so there is
no per-unit price — cleaner than forcing a rate-less `0EUR` and mixing the
field's type. The prices are illustrative placeholders.

Not every class uses every drive — a bicycle is never Diesel. But because
`Muscle` is treated as a **real** drive (it carries genuine intrinsic values:
zero emissions, no energy cost, best environmental class), every vehicle has a
drive and the structure stays uniform: each concrete vehicle inherits from
exactly one class and one drive.

---

## 7. Enums used

- **`DriveType`** — the allowed drive set. A value enum:
  `[Petrol, Diesel, Electric, NaturalGas, Hydrogen, Muscle]`.
- **`Location`** — the rental's branches, a value enum:
  `[Berlin, Hamburg, Munich, Cologne]`.
- **`Status`** — a vehicle's rental state: `[available, rented, maintenance,
  reserved]`. Default `available`, set per vehicle.
- **`EnvironmentalClass`** — the drive's environmental rating, e.g.
  `[A, B, C, D]`.
- **`LocalEmissions`** — whether the drive emits locally: `[zero, combustion]`.
- **`LicenceClass`** — EU driving-licence categories, e.g. `[AM, A1, A2, A,
  B, C1, C, CE, D]`. The field is omitted where no licence is required.

All are declared once, near the top, and referenced throughout — matching the
"admins declare standard enums at the top" pattern.

---

## 8. Decisions on record (with rationale)

- **Both templates and instances are modeled** — vehicle *classes* as
  templates, concrete *vehicles* that inherit from them.
- **Concrete vehicles differ** by `id` plus variable attributes, notably
  `location` and drive.
- **Drive is a second axis, not a mere label** — a drive brings its own
  attributes, so it is modeled as its own template the vehicle inherits from.
- **Drive templates stay vehicle-independent** — they carry only
  drive-intrinsic values, so car-Electric and bike-Electric share one template.
- **Cross-axis values live on the concrete vehicle** — `range`, `tank_capacity`,
  `charge_time`. An intermediate "class × drive" template was considered as an
  optional compression and deliberately *not* used for now (chosen: simpler,
  directly on the vehicle).
- **`Muscle` is a full drive** — giving a uniform two-axis structure rather than
  special-casing non-motorized bikes.
- **Pricing stored as components** — `base_rate` on the class, `rate_modifier`
  (a factor) on the drive. A vehicle inherits both; the app computes
  `base_rate × rate_modifier`. Nothing is baked per vehicle.
- **`deposit` and `base_rate` live on the class**, not the base, because they
  scale with the vehicle.
- **`status`, `environmental_class`, `local_emissions` are enums** (alongside
  `DriveType` and `Location`) — six value enums in total, including
  `LicenceClass`; the licence field is simply omitted where no licence is
  required (e-bike, bicycle), rather than using a "none" value.
- **Absolute emissions are vehicle-specific**; the drive carries only the
  categorical `local_emissions`.
- **Concrete vehicles are array-of-tables** (`[[vehicle : class.x, drive.y]]`),
  a fleet list identified by an `id` field, not name-addressed.
- **Cross-axis fields are drive-dependent**: `range`/`charge_time` for electric,
  `range`/`tank_capacity` for combustion, none for muscle — omitted where they
  do not apply.
- **The tank field is `tank_capacity`**, not `tank_volume`, since it is a
  volume (L) for liquids and a mass (kg) for gases.
- **Locale frame is the European Union** — licence classes, weight thresholds,
  currency, and environmental classes follow EU conventions, stated explicitly.

---

## 9. Which ATML features this exercises

- **Table inheritance**, including multi-level / transitive
  (`base → type → subclass → vehicle`).
- **Multiple inheritance** (`class + drive`, first-wins).
- **Array-of-tables inheritance** — likely for the concrete vehicles, if there
  are several of a kind (still to be confirmed; see open items).
- **Value enums** — six of them: `DriveType`, `Location`, `Status`, `EnvironmentalClass`, `LocalEmissions`, `LicenceClass`.
- **Quantities** — prices and measures will carry units, e.g. `daily_rate`,
  `deposit`, `range`, `cargo_volume`, `charge_time` (units such as EUR, km, kg,
  kWh, min — to be filled in with the attributes).
- **Template tables** — the base and the class/drive templates are pure
  templates, which will later exercise the flattener's template handling.

---

## 10. Practical consequences and scaling

Writing both versions of the same fleet makes the DRY benefit measurable. The
figures below are for a **34-vehicle fleet** — roughly a single *local* rental
branch. A regional or national operator runs hundreds or thousands of vehicles,
where the effect is far larger.

| | ATML | plain TOML |
|---|---|---|
| total lines | 353 | 680 |
| value lines (keyvals) | 216 | 608 |

Plain TOML needs about **2.8× as many value lines** for the same data, purely
from repetition it cannot factor out:

- `currency = "EUR"` appears **34 times** in TOML (once per vehicle) versus
  **once** in ATML (on the base template).
- `energy_source` appears **34 times** versus **6 times** (once per drive).
- the string `"electricity"` appears **11 times** (once per electric vehicle)
  versus **once**, in `drive.electric`.

**The gap grows with fleet size.** Every value that belongs to a class or a
drive is written once in ATML no matter how many vehicles use it; in TOML it is
repeated per vehicle. So as the fleet grows, the ATML file stays almost flat (a
few thin `[[fleet]]` entries are added), while the TOML file grows linearly. At
34 vehicles the factor is ~2.8×; for an operator with hundreds of vehicles of
each kind it approaches the number of vehicles sharing a template. This is
exactly the redundancy the project set out to remove — real, and scaling with
the size of the operation.

Two qualitative gains hold at any size, independent of line count: enums give
the fixed-choice fields (`status`, `drive_type`, `licence_class`, …) a single
checked definition instead of unvalidated repeated strings, and quantities carry
their unit as data (`350km`) rather than as a human-only comment (`350 # km`).

## 11. Open items (next steps)

- **Parent order for first-wins is deliberately left unspecified.** Class and
  drive attributes are disjoint, so first-wins never triggers here and any
  listing order yields the same result. Genuine precedence between conflicting
  sources is an application-level *filtering* concern — logic that belongs in
  code, not something the config format should encode.
- **Units surfaced by this example are now expressible** (resolved): rate
  units for `energy_cost` (`1.80EUR/L`, `0.40EUR/kWh`) via the super-unit,
  and `cargo_volume` as `m^3` via exponents. Only non-ASCII unit *symbols*
  (µ, Ω, °) remain open in the spec, and this example does not need them.
