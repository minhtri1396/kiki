package agentdef

import "fmt"

const documentWriterRepo = `This is the only repository you create and edit files in. In the agent document, docs/ means this folder. If the folder is missing or empty, stop and report. Do not create or edit files in the backend, website, or mobile repositories.
The requirement rules are this folder's AGENTS.md. Do not read an AGENTS.md from the repository root.
You write business_requirement.md and product_requirement.md in this folder.`

const designerRepo = `This is the only repository you create and edit files in. In the agent document, docs/ means this folder. If the folder is missing or empty, stop and report. Do not create or edit files in the backend, website, or mobile repositories.
The requirement folder the user names is a path inside this repository. The requirement rules are this folder's AGENTS.md. Do not read an AGENTS.md from the repository root.
You read the requirement and write design.html in that folder.`

const plannerDocumentRepo = `Read the requirement in this folder: product_requirement.md, business_requirement.md, acceptance_criteria.md when it is present, and design.html. Do not edit any file in this repository. The requirement rules are this folder's AGENTS.md. In the agent document, docs/ means this folder.`

func plannerCodeRepo(kind string) string {
	return fmt.Sprintf("Read this repository when the requirement affects %s code. Read its AGENTS.md, and for each service, that service's AGENTS.md. Do not change files, create a branch, or open a pull request here.", kind)
}

func runnerRepo(kind, others string) string {
	return fmt.Sprintf(`This is the only repository you read and change. In the agent document, %s/ means this folder. If the folder is missing or empty, stop and report. Do not read or change the %s repositories.
This folder is the git repository. Run every git and gh command here. Never create a branch, commit, or pull request in the repository root.
It is an Nx monorepo. Each project has its own folder and a project.json. One ticket can touch one or more projects.
The folder rules are this folder's AGENTS.md. The service rules are AGENTS.md inside each service the ticket changes.`, kind, others)
}

func reviewerRepo(kind, others string) string {
	return fmt.Sprintf(`This is the only repository you read. In the agent document, %s/ means this folder. If the folder is missing or empty, stop and report. Do not read or change the %s repositories.
This folder is the git repository. Check that .git exists and that git rev-parse --show-toplevel prints this folder. If it does not, stop. Run git and gh here, read-only. Do not commit, push, checkout, or merge.
It is an Nx monorepo. One pull request can touch one or more projects.
The folder rules are this folder's AGENTS.md. The service rules are AGENTS.md inside each service the pull request changes.
Read the feature's product_requirement.md and design.html from the document folder when the ticket or the pull request names that path. Do not edit the document repository.`, kind, others)
}
