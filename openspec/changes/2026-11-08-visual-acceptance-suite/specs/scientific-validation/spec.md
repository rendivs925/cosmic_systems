## ADDED Requirements

### Requirement: The required scene matrix is machine-checked

The repository SHALL carry a manifest with one entry per required visual scene,
giving the frame file, the commit it was captured at, and the capture recipe when
known. A runnable command SHALL recompute a fixed set of image statistics from
the committed bytes and SHALL exit non-zero when a scene is missing, its commit
is not in the repository history, or a statistic leaves its tolerance.

The command SHALL offer a mode that refreshes the committed baseline and a mode
that re-captures a scene from its recorded recipe and compares the fresh frame.

The statistics SHALL be coarse enough to tolerate render noise and specific
enough to catch a blank, black, mis-exposed, cloudless or geometry-collapsed
frame.

#### Scenario: Missing scene

- **WHEN** a required scene has no committed frame
- **THEN** the check exits non-zero and names that scene

#### Scenario: Regression

- **WHEN** a committed frame is replaced by one that loses the cloud deck or the
  terrain detail
- **THEN** the affected statistic leaves its tolerance and the check exits
  non-zero

#### Scenario: Stale evidence

- **WHEN** a manifest entry names a commit that is not an ancestor of `HEAD`
- **THEN** the check exits non-zero
