package agentdef

const plannerJira = `Search Jira before you create a ticket. Create and update the Epic, Stories, Tasks, and Subtasks for one requirement folder. Read issue types, fields, options, link types, and transitions from Jira. Do not invent them. Do not call Jira until the user gives a requirement folder.`

const runnerJira = `Read the parent ticket the user gives you, and its subtasks. Do not search Jira for work. Move a ticket only to a status named in the setup file, after you read its transitions. Comment on the ticket when that transition is not available.`

const reviewerJira = `Read the ticket for the pull request. For a subtask, also read the parent ticket. Move that ticket to In Review before you review, after you read its transitions. Comment on the ticket when that transition is not available. Do not set a ticket to Done.`

const plannerConfluence = `Read a Confluence page when the requirement or the user names it. Do not create, edit, or delete a page.`

const runnerConfluence = `Read a Confluence page when the ticket or the user names it. Do not create, edit, or delete a page.`

const reviewerConfluence = `Read a Confluence page when the ticket, the pull request, or the user names it. Do not create, edit, or delete a page.`

func atlassianMCPs(jira, confluence string) []Use {
	return []Use{
		{ID: "atlassian_jira", Description: jira},
		{ID: "atlassian_confluence", Description: confluence},
	}
}
