# Bull Swift MVP: Coach

Source map: Flutter `TodayView` outlook/journal/tool flows, Swift `CoachPlaceholderView`, Flutter V2 insights/resources surfaces.

MVP rule: Coach should not pretend to be an LLM until the app has a real chat backend. MVP Coach is a data-driven recommendation and journaling surface built from local metric readiness, score next actions, and evidence provenance.

## Parent View Contract

- [x] Create a dedicated `CoachView.swift` or split `CoachPlaceholderView` out of `AppShellView.swift`.
- [x] Keep this tab behind the Swift `Coach` tab item.
- [ ] Define child routes: Today Recommendation, Journal, Sleep Coach, Recovery Insights, Strain Guidance, Stress Guidance, Data Gaps.
- [x] Define a `CoachSnapshot` value type with readiness, next actions, metric highlights, missing data, and provenance.
- [x] Separate generated coaching copy from deterministic/local rule copy.
- [x] Add explicit empty state when no trusted metrics are available.
- [ ] Add previews for no-data, capture-needed, and populated days.

## Today Recommendation

- [x] Show daily readiness from `metricInputReadinessSummary()`.
- [x] Show input next action from `metricInputReadinessNextActionSummary()`.
- [x] Show score next action from `packetDerivedScoreNextActionSummary()`.
- [x] Show the primary focus: sleep, recovery, strain, stress, capture, calibration, or health sync.
- [x] Explain why using 2-4 cited local data points.
- [x] Link to the relevant Health child page.
- [x] Link to Capture when fresh device data is required.
- [x] Link to More > Health Sync when external health records are required.

## Metric Highlights

- [x] Add Sleep highlight from `todaySleepScoreSummary()`.
- [x] Add Recovery highlight from `todayRecoveryScoreSummary()`.
- [x] Add Strain highlight from `todayStrainScoreSummary()`.
- [x] Add Stress highlight from `todayStressScoreSummary()`.
- [x] Add HRV highlight from `todayHrvScoreSummary()` / `hrvFeatureSummary()`.
- [x] Add live HR highlight from `latestHeartRateSummary()` or BLE live HR.
- [x] Each highlight should show value, status, freshness, and provenance.
- [x] Hide or mark highlights whose data is sample/untrusted.

## Journal

- [ ] Add daily journal prompt from the score/action summary.
- [ ] Add optional tags from Flutter preview behavior: stressors, training, sleep quality, symptoms, recovery blockers.
- [ ] Add text note entry.
- [ ] Add save action into local store once Swift persistence exists.
- [ ] Show last saved journal entry for selected date.
- [ ] Expose journal data to Sleep/Recovery insight surfaces when available.

## Sleep Coach

- [ ] Show wind-down time.
- [ ] Show target bedtime.
- [ ] Show target wake time.
- [ ] Show sleep need fulfillment / sleep debt when available.
- [ ] Show sleep schedule from `sleepV1ScheduleSummary()`.
- [ ] Show sleep debt from `sleepV1DebtSummary()`.
- [ ] Link to Health > Sleep.
- [ ] Link to local sleep capture guidance when trusted band sleep history is needed.

## Recovery Insights

- [ ] Show recovery score and status.
- [ ] Show Resting HRV.
- [ ] Show Resting HR.
- [ ] Show provided vitals: respiratory rate, respiratory baseline, skin temp delta.
- [ ] Show missing/unavailable vitals explicitly.
- [ ] Include a deterministic recommendation based on recovery score band.
- [ ] Link to Health > Recovery trend sheet.
- [ ] Link to Health > Calibration when recovery calibration is stale or missing.

## Strain Guidance

- [ ] Show strain score and target strain.
- [ ] Show exercise duration.
- [ ] Show daytime HR.
- [ ] Show total energy.
- [ ] Show step count.
- [ ] Include guidance for under-target, in-target, and over-target strain.
- [ ] Link to Health > Strain.

## Stress Guidance

- [ ] Show stress score and status label.
- [ ] Show Last HRV.
- [ ] Show Last HR.
- [ ] Show breakdown: High, Medium, Low.
- [ ] Show non-activity stress and sleep stress when available.
- [ ] Link to Health > Stress.

## Data Gaps

- [x] Show packet input gaps from `packetDerivedFeatureNextActionSummary()`.
- [x] Show score gaps from `packetDerivedScoreNextActionSummary()`.
- [ ] Show unavailable health sync metrics from `unavailableHealthSyncMetricSummary()`.
- [ ] Show capture requirement from capture/session summaries where applicable.
- [ ] Provide one action per gap: Capture, Sync Health, Calibrate, Import Labels, Open Debug.

## Resources

- [ ] Add resource cards only as deterministic native cards, not marketing copy.
- [ ] Add Sleep resource route.
- [ ] Add Recovery score explainer route.
- [ ] Add Strain score explainer route.
- [ ] Add Stress score explainer route.
- [ ] Add Cardio Load resource route.
- [ ] Mark resource cards as static educational content.

## Future Chat Boundary

- [ ] Use `CodexCoachServer.md` as the viability boundary before enabling free-form AI Coach.
- [ ] Add placeholder protocol for future chat messages.
- [ ] Do not ship free-form AI chat until there is a backend, privacy policy, and persistence strategy.
- [ ] Keep MVP "Ask Coach" input disabled or route to deterministic suggested questions.
- [ ] Log selected suggested questions as UI actions only.

## Parallel Agent Tasks

- [x] Agent Coach-A: Extract Coach tab and build `CoachSnapshot`.
- [x] Agent Coach-B: Implement Today Recommendation and Metric Highlights.
- [ ] Agent Coach-C: Implement Journal entry UI and local persistence hooks.
- [ ] Agent Coach-D: Implement Sleep Coach.
- [ ] Agent Coach-E: Implement Recovery/Strain/Stress guidance cards.
- [ ] Agent Coach-F: Implement Data Gaps and action routing.
- [ ] Agent Coach-G: Add deterministic resources and future-chat boundary UI.
- [ ] Agent Coach-H: Add previews and screenshot verification.

## Acceptance Checks

- [ ] Coach never shows invented metrics.
- [ ] Every recommendation cites the local metric or gap that caused it.
- [ ] Empty state tells the user the next concrete action.
- [ ] Journal can be used without a connected device.
- [ ] Coach links back to Health/Home/More without circular navigation bugs.
