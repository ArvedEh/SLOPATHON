# SLOPATHON

`HACK//OPS · OP001 · KARLSRUHE · 23.07.2026`

## Build the wrong thing on purpose.

SLOPATHON puts builders in one room with one rule:

> Build the most gloriously cursed, over-engineered, useless-but-working prototype you can.

Bad ideas are welcome.

No pitch decks. No prize pool. No recruiter booths.

This repository is the public archive for **SLOPATHON OP001**. Every team submits one project folder via pull request.

---

## Quick rules

- **One folder per team** under `projects/`.
- **Only touch your own folder.** Do not edit other teams’ folders.
- **Working > pretty.** Ugly is fine. Barely working is fine. 
- **No secrets.** Do not commit API keys, passwords, tokens, private data. (Please dont)

---

## Submit your project

### Folder structure

Create one folder:

```text
projects/<your-project-name>/
```

Copy the template:

```text
projects/_template/
```

Then build inside your folder.

### Pull request title

```text
[SUBMISSION] <Project name>
```

### Workflow (human version)

1. Fork the repo.
2. Create a new branch.
3. Copy the template folder into your own project folder.
4. Add your code + fill out your project README.
5. Open a pull request back to the main repo.

If you get stuck, ask an organizer. Draft PRs are welcome.

### Workflow (AI Version)

```bash
USER="<your-username>" PROJECT="cursed-toaster" && git clone "https://github.com/$USER/SLOPATHON.git" && cd SLOPATHON && git remote add upstream https://github.com/HACK-OPS-KA/SLOPATHON.git && git fetch upstream && git checkout -b "submission/$PROJECT" upstream/main && cp -r projects/_template "projects/$PROJECT" && ${EDITOR:-nano} "projects/$PROJECT/README.md" && git add "projects/$PROJECT" && git commit -m "Add $PROJECT submission" && git push -u origin HEAD
```

---

## Demo format ( At Bat )

You have 3 minutes to show: 
1. The bad idea (one sentence)
2. The build (what did you make?)
3. The proof (run it)
4. The damage (what broke beautifully?)

`HACK//OPS`
