# SLiM-CC — Nexus Mod Collection Analysis

Status: research + planning draft  
Scope: analysis only. **No automatic download path is planned in this phase.**

## Goal

SLiM-CC should accept a Nexus Mods Collection link, extract the underlying mod references, and turn them into a structured, reviewable list of:

- Nexus mod page links
- optional file links / file IDs when present
- external resources that are part of the collection
- collection metadata useful for later manual import or validation

The first version is an **analysis tool**, not a downloader.

## What the official Nexus docs imply

From the official Nexus help and app docs:

- A collection is a meta file that references a list of mods; it is not a bundle of actual mod files.
- Collections may also include bundled assets, but those are meant for tool-generated files, not mod redistribution.
- External resources can be linked in a collection and must be presented as links to the user.
- Nexus states that collections are meant to be installed through Vortex, and third-party tools may use an open GraphQL API.
- The official Nexus app docs say Premium enables one-click collection downloads; free users must go through mod rows individually.
- Nexus API access is rate limited, and applications are expected to identify themselves properly.

Relevant research sources:

- https://help.nexusmods.com/article/115-guidelines-for-collections
- https://nexus-mods.github.io/NexusMods.App/users/gettingstarted/DownloadACollection/
- https://help.nexusmods.com/article/114-api-acceptable-use-policy
- https://help.nexusmods.com/article/105-i-have-reached-a-daily-or-hourly-limit-api-requests-have-been-consumed-rate-limit-exceeded-what-does-this-mean
- https://github.com/Nexus-Mods/node-nexus-api

## Product decision

SLiM-CC should do the following:

1. Accept a collection URL or collection slug.
2. Resolve the collection metadata.
3. Extract the mod references and external-resource references.
4. Show the result in a review screen.
5. Optionally map the references to already known local mods or Nexus metadata.

SLiM-CC should **not**:

- automatically download collection content in this phase
- attempt to bypass Nexus Premium restrictions
- assume every collection reference can be turned into a direct download

Premium-only automatic downloading is a separate later feature and should remain gated behind:

- explicit Nexus account support
- explicit premium detection or equivalent server-side confirmation
- clear user consent
- a separate implementation and test pass

## Recommended approach

### Option 1: HTML/DOM collection extraction

Parse the public collection page and extract visible mod links and labels.

Pros:
- simplest to prototype
- no dependence on a hidden schema
- works well as a fallback when API fields change

Cons:
- fragile if the page layout changes
- less structured metadata
- weaker support for future automation

### Option 2: API-first GraphQL collection resolver

Use the Nexus API / GraphQL path to resolve collection metadata and then normalize the mod list from structured fields.

Pros:
- best data quality
- easier to cache and compare
- better foundation for later validation and premium-gated download logic

Cons:
- depends on Nexus API stability and registration details
- may require more upfront work to understand collection schema variants

### Option 3: Hybrid resolver with fallback

Try API/GraphQL first, then fall back to page extraction if the API shape is incomplete or unavailable.

Pros:
- most resilient
- good long-term fit
- keeps the analysis feature usable even if Nexus changes one layer

Cons:
- more code than a single-strategy parser
- needs careful precedence rules

### Recommendation

Use **Option 3**.

That gives SLiM-CC a structured default path while keeping the feature usable if the collection schema or API exposure changes.

## Proposed behavior

### Input

- Nexus collection URL
- optional plain collection slug
- optional game context if the URL is ambiguous

### Output

- collection title
- collection author / curator if available
- collection description / notes if available
- list of Nexus mod references
- list of external-resource references
- list of references that could not be resolved cleanly
- warnings for duplicates, missing game context, or unsupported entries

### Resolution rules

- If a reference can be mapped to a Nexus mod page, store the collection item as a Nexus link reference.
- If the collection references an external resource, preserve it as an external link and do not pretend it is a Nexus mod.
- If a reference contains a file identifier, keep both the mod page and file identity when available.
- If the source data is incomplete, keep the raw reference so the user can inspect it manually.

## Implementation phases

### Phase 1: URL handling and normalization

- detect Nexus collection URLs
- normalize canonical collection slugs / identifiers
- store raw source URL

### Phase 2: metadata extraction

- fetch collection title, author, game context, and visible notes
- resolve the list of collection entries
- classify entries into Nexus mods vs external resources

### Phase 3: review UI

- show the extracted list in a compact table
- allow copy/open actions per row
- highlight unresolved or external entries
- keep this review-only, with no automatic download button yet

### Phase 4: local mapping support

- compare extracted mod references against already imported local mods
- mark already-installed or already-known items
- expose mismatches and missing local counterparts

### Phase 5: later premium-gated automation

- only after Nexus-side clarification
- only for premium users
- separate command path and separate confirmation flow
- no silent background bulk download

## Risks and constraints

- Nexus collection documentation is less explicit than the normal mod/file API surface.
- The app must not confuse collection analysis with download automation.
- Rate limits matter even for analysis, because large collections can trigger many metadata lookups.
- Free users must still be able to inspect a collection and open the referenced Nexus pages manually.
- Automatic download must stay off until the Nexus-side rules are fully understood and implemented.

## Acceptance criteria for the analysis feature

- A collection URL can be entered and resolved.
- The app produces a structured list of collection entries.
- Nexus mod links are extracted where possible.
- External-resource links are preserved.
- No automatic download is attempted.
- The result is visible in UI and can be reviewed before any later action.

## Open questions for the later download phase

- Which Nexus API fields expose the full collection graph reliably?
- Which collection items are available only through the browser page and not the API?
- How does Nexus want premium-gated collection downloading to be verified from a third-party app?
- Which part of the collection download flow is allowed for non-premium users, if any?
- How should SLiM-CC handle collections that mix Nexus-hosted mods and external resources?

