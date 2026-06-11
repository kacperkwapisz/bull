# Coach LLM decision (manual gate)

**Status:** Working default for alpha — re-run manual smoke before changing production env.

## Production default (CoachAPI env)

| Role | Provider | Model id |
|------|----------|----------|
| Default (`model_tier: default`) | Groq (or OpenCode Zen for dev) | `openai/gpt-oss-120b` |
| Deep (`model_tier: deep`) | Same | `openai/gpt-oss-120b` (swap to Sonnet/4o if manual smoke shows gap) |

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
- Changing default upstream model or host