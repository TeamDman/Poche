# GitHub project head and Pages policy

- Status: Accepted and applied
- Date: 2026-08-03 (America/Toronto)
- Repository: `TeamDman/Poche`

## Applied state

- The default branch is `model-checking`.
- The prior `main` branch is retained; it was not force-pushed, rewritten, or
  deleted.
- GitHub Pages exists at <https://teamdman.github.io/Poche/> with
  `build_type = workflow` and HTTPS enforcement.
- The `github-pages` environment uses custom branch policies and permits only
  the `model-checking` branch.
- Repository Actions are enabled for all actions. The default workflow token is
  read-only and cannot approve pull-request reviews; deployment workflows must
  request only their required `pages: write` and `id-token: write` permissions.

## Pre-change evidence

- Previous default branch: `main`
- Open pull requests: none
- Repository rulesets: none
- Branch protection on `main`: none
- Existing Pages site: none (the API returned HTTP 404)
- Existing environments: none
- Preserved `main` head: `0a19283bf949c3adf9d961f01114631cf44960eb`
- First applied `model-checking` head:
  `765cb9cab9cdfe40b4f0ee826a41372d0d62be98`

The absence of pre-existing protection and open pull requests meant the default
change did not bypass a review rule or retarget active work. It does not justify
deleting `main`; that would require a separate explicit decision.

## Rollback

If `model-checking` must stop being the project head:

1. Verify `main` still exists and select the intended rollback commit.
2. Run `gh repo edit TeamDman/Poche --default-branch main`.
3. Read the current Pages environment branch-policy ID from
   `repos/TeamDman/Poche/environments/github-pages/deployment-branch-policies`.
4. Delete only that specific deployment-branch policy, then create a `main`
   branch policy if Pages should deploy from `main`.
5. Re-read the repository, Pages, and environment APIs before announcing the
   rollback.

Changing the default branch does not merge, delete, or rewrite either branch.
