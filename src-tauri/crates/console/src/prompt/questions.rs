//! How an agent sends questions back to the user. The launch prompt states the block, and the
//! transcript parser reads that same block from the agent's reply.

/// Tells the agent to ask the user with one questions block, then wait.
pub(super) fn write(b: &mut String) {
    b.push('\n');
    b.push_str(
        "Questions for the user. When you need the user to answer before you continue, \
         stop and send one questions block, then wait. Do not guess.\n\n",
    );
    b.push_str(
        "Each question starts with Q: and a short name. The next line says what you need. \
         Then write A: text when the user should type an answer, or A: choice and one option \
         per line starting with \"- \". After that you may add S: and a suggested answer. \
         For A: text, S: is text the user can accept or edit. For A: choice, S: is one of the options. \
         One block can hold more than one question. \
         The user answers each question as \"Name: value\", one per line.\n\n",
    );
    b.push_str(
        "Q: Ticket\nThe Jira key.\nA: text\nS: PROJ-12\n\nQ: Scope\nWhat should this cover?\nA: choice\n- Website only\n- Website and mobile\nS: Website only\n\n",
    );
    b.push_str("The block begins with this line:\n===== QUESTIONS =====\n");
    b.push_str("The block ends with this line:\n===== END QUESTIONS =====\n");
}
