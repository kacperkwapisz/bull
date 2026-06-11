# Coach LLM decision (manual gate)

**Status:** Working default for alpha — re-run manual smoke before changing production env.

Client exposes two tiers via `CoachModelPreset` (persisted, chosen in chat profile menu under "Model"):

- `coach` (default tier) — fast daily use
- `deeperInsight` (deep tier) — stronger reasoning

The iOS client sends `model_tier` ("default" | "deep"); CoachAPI resolves the concrete model string from env per tier. Actual LLM is never chosen in the app binary.

## Production default (CoachAPI env)

| Role | Provider | Model id |
|------|----------|----------|
| Default (`model_tier: default`) | oraiapi.com (https://oraiapi.com/v1) | `gpt-oss-120b` |
| Deep (`model_tier: deep`) | Same | `gpt-oss-120b` (swap to a stronger model on the same or different compatible upstream if manual smoke shows quality gap for complex prompts) |

oraiapi.com was selected as the working upstream for alpha (OpenAI chat completions + tools support). Dev key: `dev-key`. Client still uses the two-tier preset picker ("Coach" vs "Deeper insight") and the server resolves the concrete model.

## Dev / zero-cost upstream

OpenCode Zen free models (`https://opencode.ai/zen/v1/chat/completions`) — **dev only**, not TestFlight production default.

## Manual smoke checklist (8 prompts)

1. `What is blocking today's scores?`
2. `Summarize my recovery signals and what is missing.`
3. Home tip prompt from `CoachTipFactory.homeTip`
4. Sleep / recovery / strain metric tips
5. Adversarial: claim 99% recovery without tool support
6. Confirm tools run locally (metric lines appear in DEBUG chat)
7. Confirm no Codex URLs in app binary sources
8. `gappy_day`-style context: coach must cite missing data

## Re-run when

- Changing `COACH_SYSTEM_INSTRUCTIONS` or tool schema
- Changing default upstream model or host (per-tier `COACH_MODEL_*`)
- Adding new tiers or altering `CoachModelPreset` mapping