/**
 * Default catalog of journal behaviors the app offers out of the box. Users
 * curate which ones they track and can add their own custom tags on top.
 *
 * Each tag is a stable snake_case identifier (used as the analysis key) plus a
 * human label and a category for grouping in the picker. A few tags carry an
 * amount (dose / count) because magnitude matters for them; the rest are yes/no.
 *
 * These are generic wellbeing habits; nothing here is specific to any other
 * product. Tags are the unit the insight engine groups by.
 */

export type CatalogCategory =
  | "substances"
  | "nutrition"
  | "sleep"
  | "mind"
  | "activity"
  | "lifestyle"

export interface CatalogTag {
  tag: string
  label: string
  category: CatalogCategory
  /** When true, the entry accepts an amount alongside the yes/no flag. */
  hasAmount?: boolean
  /** Unit hint for amount-bearing tags (display only). */
  unit?: string
  /** Whether Bull can derive this from the band instead of manual logging. */
  autoSource?: "band"
}

export const JOURNAL_CATALOG: CatalogTag[] = [
  // Substances
  { tag: "alcohol", label: "Alcohol", category: "substances", hasAmount: true, unit: "drinks" },
  { tag: "caffeine", label: "Caffeine", category: "substances", hasAmount: true, unit: "mg" },
  { tag: "late_caffeine", label: "Caffeine late in the day", category: "substances" },
  { tag: "nicotine", label: "Nicotine", category: "substances" },
  { tag: "cannabis", label: "Cannabis", category: "substances" },

  // Nutrition
  { tag: "late_meal", label: "Ate late", category: "nutrition" },
  { tag: "large_meal", label: "Large meal", category: "nutrition" },
  { tag: "high_sugar", label: "High sugar", category: "nutrition" },
  { tag: "processed_food", label: "Highly processed food", category: "nutrition" },
  { tag: "hydrated", label: "Well hydrated", category: "nutrition" },
  { tag: "supplements", label: "Took supplements", category: "nutrition" },

  // Sleep hygiene
  { tag: "screens_before_bed", label: "Screens before bed", category: "sleep" },
  { tag: "read_before_bed", label: "Read before bed", category: "sleep" },
  { tag: "dark_room", label: "Dark room", category: "sleep" },
  { tag: "cool_room", label: "Cool room", category: "sleep" },
  { tag: "consistent_bedtime", label: "Consistent bedtime", category: "sleep" },
  { tag: "nap", label: "Napped", category: "sleep" },

  // Mind
  { tag: "meditation", label: "Meditated", category: "mind" },
  { tag: "high_stress", label: "Stressful day", category: "mind" },
  { tag: "anxious", label: "Felt anxious", category: "mind" },
  { tag: "social_time", label: "Social time", category: "mind" },
  { tag: "alone_time", label: "Time alone", category: "mind" },

  // Activity
  { tag: "workout", label: "Worked out", category: "activity", autoSource: "band" },
  { tag: "steps_10k", label: "10k+ steps", category: "activity", autoSource: "band" },
  { tag: "sauna", label: "Sauna", category: "activity" },
  { tag: "cold_exposure", label: "Cold exposure", category: "activity" },
  { tag: "stretching", label: "Stretched / mobility", category: "activity" },
  { tag: "sore", label: "Sore / DOMS", category: "activity" },

  // Lifestyle
  { tag: "travel", label: "Travel", category: "lifestyle" },
  { tag: "sick", label: "Felt sick", category: "lifestyle" },
  { tag: "work_late", label: "Worked late", category: "lifestyle" },
  { tag: "outdoors", label: "Time outdoors", category: "lifestyle" },
]
