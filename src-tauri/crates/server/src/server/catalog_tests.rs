//! The places and names Settings fills in for an agent.

use kiki_agentdef as agentdef;
use kiki_mcp as mcp;

use super::catalog::apply_atlassian_places;

#[test]
fn apply_atlassian_places_uses_settings_until_the_agent_chooses() {
    let cfg = mcp::Atlassian {
        jira: mcp::Service {
            board_id: "12".into(),
            board_name: "KIKI".into(),
            ..Default::default()
        },
        confluence: mcp::Service {
            space_id: "111".into(),
            space_key: "ENG".into(),
            space_name: "Engineering".into(),
            parent_id: "99".into(),
            parent_title: "Requirements".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let uses = vec![
        agentdef::Use {
            id: "atlassian_jira".into(),
            ..Default::default()
        },
        agentdef::Use {
            id: "atlassian_confluence".into(),
            space_id: "222".into(),
            space_key: "OPS".into(),
            space_name: "Operations".into(),
            ..Default::default()
        },
    ];
    let got = apply_atlassian_places(uses, &cfg);
    assert!(
        got[0].board_id == "12" && got[0].board_name == "KIKI",
        "jira = {:?}",
        got[0]
    );
    assert!(
        got[1].space_key == "OPS" && got[1].parent_title.is_empty(),
        "confluence = {:?}",
        got[1]
    );
    let chosen = apply_atlassian_places(
        vec![agentdef::Use {
            id: "atlassian_jira".into(),
            board_id: "14".into(),
            board_name: "Platform".into(),
            ..Default::default()
        }],
        &cfg,
    );
    assert_eq!(chosen[0].board_name, "Platform", "override = {:?}", chosen[0]);
}
