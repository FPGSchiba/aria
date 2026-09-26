# Sprint Guideline — ARIA

Companion to `docs/backlog/jira-rulebook.md`. Sprints are planned one at a time, on demand.
Nothing is scheduled beyond the sprint being planned; the roadmap in
`docs/backlog/roadmap.md` says what comes next, not when.

Board 170. Jira assigns sprint ids; record each one in the sprint's plan when it is created.

## The sprint goal

One sentence, describing something that **runs** at the end of the sprint. Not a list of
components — a thing that can be started and used.

Good: "A typed message from the CLI client reaches the hosted LLM and streams back."
Bad: "Implement the Gateway and the Agent Core."

If the goal cannot be phrased as something runnable, the sprint is cut along the wrong axis.

## Composition rules

- **One theme is the focus.** Stories from other themes only where the focus theme needs them.
- **At most one Spike.** Research has unpredictable length; two spikes can consume a sprint.
- **No story larger than 5 points.** An 8 is split during planning or left out.
- **Every sprint ends demoable.** If the last story lands and nothing new can be run, the sprint
  was a layer, not an increment.
- Every story meets the Definition of Ready before the sprint starts.

## Open questions — decide at the last responsible moment

Leaving a question open is a choice, not a gap. A question is answered in the sprint **before**
the first story that needs it — not earlier, when less is known, and not later, when that story
would stall.

- **Answerable by experiment or measurement** → a Spike in that earlier sprint. It uses the
  sprint's one Spike slot and Blocks the story that needs the answer.
- **A question of preference** → decided at the planning of the sprint that needs it.
- **Until then**, a story may carry an interim only if reversing it later is cheap. An interim
  that would be expensive to undo makes the story not Ready — it waits for the answer.

Spikes are created at the planning of the sprint that runs them, never ahead of it.

## Capacity

Capacity is the previous sprint's completed points, not an ambition. For the first sprint, pick
a deliberately low number and correct afterwards — the first estimate is a guess and treating it
as a commitment produces permanent carry-over.

Sprint length: two weeks. Availability is part-time and irregular, so a longer sprint hides
problems and a shorter one is mostly ceremony.

## Planning checklist

1. Read the roadmap and pick the focus theme — the earliest one whose "ready when" gate is met.
2. Write the sprint goal sentence first, before selecting stories.
3. Pull or write stories that serve that goal, per the rulebook.
4. Check each against the Definition of Ready.
5. For each open item, keep its interim only if it is cheap to reverse; otherwise hold the story
   and use this sprint's Spike to answer the question for the next one.
6. Sum the points against capacity. Cut the least essential story, never shrink the criteria.
7. Confirm: if every story lands, can the goal sentence be demonstrated?
8. Create the sprint and move the issues in.

## Creating a sprint

The Atlassian MCP cannot create sprints. Use Claude in Chrome with the Agile API:

- `POST /rest/agile/1.0/sprint` with `originBoardId: 170`, name, and dates.
- Move issues with `POST /rest/agile/1.0/sprint/{sprintId}/issue`.
- Start the sprint from the board UI.

Name sprints `ARIA Sprint <n> — <goal in three or four words>`.

## Carry-over

An unfinished story returns to the backlog and is **re-estimated** at the next planning. It is
not silently continued: a story that survives two sprints is a story that was described wrongly,
and re-estimating is what surfaces that.

## Closing a sprint

- Verify each closed story against its acceptance criteria — not against the impression that it
  is finished.
- Record completed points as the next sprint's capacity.
- Update the affected theme's status in `docs/backlog/roadmap.md`.
- If the sprint surfaced a decision, it goes to `docs/decisions/` with a D-number before the next
  planning, so the next sprint's stories can rely on it.
