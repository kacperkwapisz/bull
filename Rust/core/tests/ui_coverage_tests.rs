use std::{fs, path::Path};

use bull_core::ui_coverage::{
    LayoutCoverageRule, NavigationCoverageRule, SourceClassCoverageRule, UiCoverageAuditInput,
    UiCoverageExpectedInventory, UiCoverageInventoryPaths, UiCoverageRules, UiCoverageStatus,
    UiResourceCoverageRule, run_ui_coverage_audit,
};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

#[test]
fn ui_coverage_audit_passes_when_every_surface_has_status() {
    let dir = write_inventory(
        "graph,type,id,label,class_or_graph,deep_link_count,deep_links,argument_count,arguments,path\n\
         home_graph,fragment,home_fragment,\"Home, Main\",com.whoop.home.view.HomeFragment,0,,0,,res/navigation/home.xml\n\
         community_graph,fragment,communityFragment,CommunityFragment,com.whoop.community.CommunityFragment,0,,0,,res/navigation/community.xml\n",
        "resource,variant,category,root_tag,path\nfragment_home,layout,fragment,FrameLayout,res/layout/fragment_home.xml\nactivity_main,layout,activity,FrameLayout,res/layout/activity_main.xml\n",
        "class_name,category,module,package_path,path\nHomeFragment,fragment,home,com/whoop/home,sources/HomeFragment.java\nCommunityFragment,fragment,community,com/whoop/community,sources/CommunityFragment.java\n",
    );
    let input = input_with_rules();

    let report = run_ui_coverage_audit(&input, dir.path()).unwrap();

    assert!(report.pass, "{:?}", report.issues);
    assert!(report.inventory_valid);
    assert!(report.coverage_map_valid);
    assert!(report.all_surfaces_classified);
    assert!(!report.has_deferred_review_debt);
    assert_eq!(report.navigation.total_count, 2);
    assert_eq!(report.layouts.covered_count, 2);
    assert_eq!(report.ui_resources.covered_count, 2);
    assert_eq!(report.source_classes.covered_count, 2);
    assert_eq!(report.navigation.status_counts["implement"], 1);
    assert_eq!(report.navigation.status_counts["omit"], 1);
    assert_eq!(report.navigation.deferred_count, 0);
    assert!(report.navigation.deferred_surfaces.is_empty());
}

#[test]
fn missing_navigation_coverage_fails_with_specific_surface() {
    let dir = write_inventory(
        "graph,type,id,label,class_or_graph,deep_link_count,deep_links,argument_count,arguments,path\n\
         home_graph,fragment,home_fragment,Home,com.whoop.home.view.HomeFragment,0,,0,,res/navigation/home.xml\n\
         community_graph,fragment,communityFragment,CommunityFragment,com.whoop.community.CommunityFragment,0,,0,,res/navigation/community.xml\n\
         unknown_graph,fragment,unknownFragment,Unknown,com.whoop.UnknownFragment,0,,0,,res/navigation/unknown.xml\n",
        "resource,variant,category,root_tag,path\nfragment_home,layout,fragment,FrameLayout,res/layout/fragment_home.xml\nactivity_main,layout,activity,FrameLayout,res/layout/activity_main.xml\n",
        "class_name,category,module,package_path,path\nHomeFragment,fragment,home,com/whoop/home,sources/HomeFragment.java\nCommunityFragment,fragment,community,com/whoop/community,sources/CommunityFragment.java\n",
    );
    let input = input_with_rules();

    let report = run_ui_coverage_audit(&input, dir.path()).unwrap();

    assert!(!report.pass);
    assert!(report.inventory_valid);
    assert!(report.coverage_map_valid);
    assert!(!report.all_surfaces_classified);
    assert!(!report.has_deferred_review_debt);
    assert_eq!(report.navigation.missing_count, 1);
    assert!(
        report
            .issues
            .contains(&"navigation_missing_coverage:unknown_graph:unknownFragment".to_string())
    );
    assert!(report.navigation.next_actions.iter().any(|action| {
        action.scope == "navigation:unknown_graph:unknownFragment"
            && action.reason == "navigation_missing_coverage"
            && action.action.contains("Add a coverage-map rule")
    }));
    assert!(report.next_actions.iter().any(|action| {
        action.scope == "navigation:unknown_graph:unknownFragment"
            && action.reason == "navigation_missing_coverage"
    }));
}

#[test]
fn missing_ui_resource_coverage_fails_with_specific_resource() {
    let dir = write_inventory_with_resources(
        "graph,type,id,label,class_or_graph,deep_link_count,deep_links,argument_count,arguments,path\n\
         home_graph,fragment,home_fragment,Home,com.whoop.home.view.HomeFragment,0,,0,,res/navigation/home.xml\n\
         community_graph,fragment,communityFragment,CommunityFragment,com.whoop.community.CommunityFragment,0,,0,,res/navigation/community.xml\n",
        "resource,variant,category,root_tag,path\nfragment_home,layout,fragment,FrameLayout,res/layout/fragment_home.xml\nactivity_main,layout,activity,FrameLayout,res/layout/activity_main.xml\n",
        "type,variant,resource,path,bytes\nlayout,layout,fragment_home,res/layout/fragment_home.xml,128\nmenu,menu,unmapped_overflow,res/menu/unmapped_overflow.xml,64\n",
        "class_name,category,module,package_path,path\nHomeFragment,fragment,home,com/whoop/home,sources/HomeFragment.java\nCommunityFragment,fragment,community,com/whoop/community,sources/CommunityFragment.java\n",
    );
    let input = input_with_rules();

    let report = run_ui_coverage_audit(&input, dir.path()).unwrap();

    assert!(!report.pass);
    assert!(report.inventory_valid);
    assert!(report.coverage_map_valid);
    assert!(!report.all_surfaces_classified);
    assert_eq!(report.ui_resources.missing_count, 1);
    assert!(
        report
            .issues
            .contains(&"ui_resource_missing_coverage:menu:unmapped_overflow".to_string())
    );
    assert!(report.ui_resources.next_actions.iter().any(|action| {
        action.scope == "ui_resource:menu:unmapped_overflow"
            && action.reason == "ui_resource_missing_coverage"
            && action.action.contains("Add a coverage-map rule")
    }));
}

#[test]
fn omitted_or_deferred_rules_require_a_reason() {
    let dir = write_inventory(
        "graph,type,id,label,class_or_graph,deep_link_count,deep_links,argument_count,arguments,path\ncommunity_graph,fragment,communityFragment,CommunityFragment,com.whoop.community.CommunityFragment,0,,0,,res/navigation/community.xml\n",
        "resource,variant,category,root_tag,path\nfragment_home,layout,fragment,FrameLayout,res/layout/fragment_home.xml\n",
        "class_name,category,module,package_path,path\nHomeFragment,fragment,home,com/whoop/home,sources/HomeFragment.java\n",
    );
    let mut input = input_with_rules();
    input.coverage.navigation[1].reason = None;

    let report = run_ui_coverage_audit(&input, dir.path()).unwrap();

    assert!(!report.pass);
    assert!(report.inventory_valid);
    assert!(!report.coverage_map_valid);
    assert!(report.all_surfaces_classified);
    assert!(
        report
            .issues
            .contains(&"navigation_rule_reason_required:nav-community-omit".to_string())
    );
    assert!(report.next_actions.iter().any(|action| {
        action.scope == "nav-community-omit"
            && action.reason == "navigation_rule_reason_required"
            && action.action.contains("Add the omit/defer reason")
    }));
}

#[test]
fn rules_without_selectors_are_rejected() {
    let dir = write_inventory(
        "graph,type,id,label,class_or_graph,deep_link_count,deep_links,argument_count,arguments,path\nhome_graph,fragment,home_fragment,Home,com.whoop.home.view.HomeFragment,0,,0,,res/navigation/home.xml\n",
        "resource,variant,category,root_tag,path\nfragment_home,layout,fragment,FrameLayout,res/layout/fragment_home.xml\n",
        "class_name,category,module,package_path,path\nHomeFragment,fragment,home,com/whoop/home,sources/HomeFragment.java\n",
    );
    let mut input = input_with_rules();
    input.coverage.navigation[0].graph = None;
    input.coverage.navigation[0].destination_id = None;
    input.coverage.navigation[0].class_prefix = None;

    let report = run_ui_coverage_audit(&input, dir.path()).unwrap();

    assert!(!report.pass);
    assert!(report.inventory_valid);
    assert!(!report.coverage_map_valid);
    assert!(report.all_surfaces_classified);
    assert!(
        report
            .issues
            .contains(&"navigation_rule_selector_required:nav-home-implement".to_string())
    );
}

#[test]
fn stale_rules_that_match_no_surfaces_are_reported() {
    let dir = write_inventory(
        "graph,type,id,label,class_or_graph,deep_link_count,deep_links,argument_count,arguments,path\nhome_graph,fragment,home_fragment,Home,com.whoop.home.view.HomeFragment,0,,0,,res/navigation/home.xml\n",
        "resource,variant,category,root_tag,path\nfragment_home,layout,fragment,FrameLayout,res/layout/fragment_home.xml\n",
        "class_name,category,module,package_path,path\nHomeFragment,fragment,home,com/whoop/home,sources/HomeFragment.java\n",
    );
    let input = input_with_rules();

    let report = run_ui_coverage_audit(&input, dir.path()).unwrap();

    assert!(!report.pass);
    assert!(report.inventory_valid);
    assert!(!report.coverage_map_valid);
    assert!(report.all_surfaces_classified);
    assert!(
        report.issues.contains(
            &"coverage_rule_matched_no_surfaces:navigation:nav-community-omit".to_string()
        )
    );
    assert!(report.next_actions.iter().any(|action| {
        action.scope == "navigation:nav-community-omit"
            && action.reason == "coverage_rule_matched_no_surfaces"
            && action.action.contains("Remove the stale coverage rule")
    }));
}

#[test]
fn deferred_surfaces_are_reported_as_review_debt_without_becoming_missing() {
    let dir = write_inventory(
        "graph,type,id,label,class_or_graph,deep_link_count,deep_links,argument_count,arguments,path\n\
         strength_trainer_graph,fragment,strengthTrainerFragment,Strength,com.whoop.strength.StrengthTrainerFragment,0,,0,,res/navigation/strength.xml\n",
        "resource,variant,category,root_tag,path\nfragment_browser,layout,browser,FrameLayout,res/layout/fragment_browser.xml\n",
        "class_name,category,module,package_path,path\nOtherUiWidget,other_ui,other,com/whoop/other,sources/OtherUiWidget.java\n",
    );
    let input = UiCoverageAuditInput {
        schema: "bull.ui-coverage-audit.v1".to_string(),
        inventory: UiCoverageInventoryPaths {
            navigation_destinations_csv: "navigation-destinations.csv".to_string(),
            layouts_csv: "layouts.csv".to_string(),
            ui_resources_csv: "ui-resources.csv".to_string(),
            source_ui_classes_csv: "source-ui-classes.csv".to_string(),
        },
        expected_inventory: None,
        coverage: UiCoverageRules {
            navigation: vec![NavigationCoverageRule {
                rule_id: "nav-strength-defer".to_string(),
                graph: Some("strength_trainer_graph".to_string()),
                destination_id: None,
                destination_type: None,
                class_or_graph: None,
                class_prefix: None,
                status: UiCoverageStatus::Defer,
                bull_area: "Activity".to_string(),
                target_level: None,
                reason: Some("Manual local activity UX mapping is pending.".to_string()),
            }],
            layouts: vec![LayoutCoverageRule {
                rule_id: "layout-browser-defer".to_string(),
                resource: None,
                category: Some("browser".to_string()),
                status: UiCoverageStatus::Defer,
                bull_area: "Content".to_string(),
                target_level: None,
                reason: Some("Embedded content surfaces need manual review.".to_string()),
            }],
            ui_resources: vec![UiResourceCoverageRule {
                rule_id: "ui-resource-layout-defer".to_string(),
                resource_type: Some("layout".to_string()),
                variant: None,
                resource: None,
                status: UiCoverageStatus::Defer,
                bull_area: "Coverage Review".to_string(),
                target_level: None,
                reason: Some("Layout resource file family needs screen mapping.".to_string()),
            }],
            source_classes: vec![SourceClassCoverageRule {
                rule_id: "source-other-ui-defer".to_string(),
                class_name: None,
                category: Some("other_ui".to_string()),
                module: None,
                status: UiCoverageStatus::Defer,
                bull_area: "Coverage Review".to_string(),
                target_level: None,
                reason: Some("Miscellaneous UI class needs module-level review.".to_string()),
            }],
        },
    };

    let report = run_ui_coverage_audit(&input, dir.path()).unwrap();

    assert!(report.pass, "{:?}", report.issues);
    assert!(report.inventory_valid);
    assert!(report.coverage_map_valid);
    assert!(report.all_surfaces_classified);
    assert!(report.has_deferred_review_debt);
    assert_eq!(report.navigation.missing_count, 0);
    assert_eq!(report.navigation.deferred_count, 1);
    assert_eq!(
        report.navigation.deferred_surfaces[0].key,
        "strength_trainer_graph:strengthTrainerFragment"
    );
    assert_eq!(
        report.navigation.deferred_surfaces[0].rule_id,
        "nav-strength-defer"
    );
    assert_eq!(
        report.navigation.deferred_surfaces[0].reason,
        "Manual local activity UX mapping is pending."
    );
    assert_eq!(report.layouts.deferred_count, 1);
    assert_eq!(report.ui_resources.deferred_count, 2);
    assert_eq!(report.source_classes.deferred_count, 1);
    assert!(report.navigation.next_actions.iter().any(|action| {
        action.scope == "navigation:strength_trainer_graph:strengthTrainerFragment"
            && action.reason == "deferred_review_debt"
    }));
    assert!(report.next_actions.iter().any(|action| {
        action.reason == "deferred_review_debt"
            && action.action.contains("Review deferred navigation")
    }));
}

#[test]
fn expected_inventory_fingerprint_passes_when_counts_and_checksums_match() {
    let dir = write_inventory(
        "graph,type,id,label,class_or_graph,deep_link_count,deep_links,argument_count,arguments,path\n\
         home_graph,fragment,home_fragment,\"Home, Main\",com.whoop.home.view.HomeFragment,0,,0,,res/navigation/home.xml\n\
         community_graph,fragment,communityFragment,CommunityFragment,com.whoop.community.CommunityFragment,0,,0,,res/navigation/community.xml\n",
        "resource,variant,category,root_tag,path\nfragment_home,layout,fragment,FrameLayout,res/layout/fragment_home.xml\nactivity_main,layout,activity,FrameLayout,res/layout/activity_main.xml\n",
        "class_name,category,module,package_path,path\nHomeFragment,fragment,home,com/whoop/home,sources/HomeFragment.java\nCommunityFragment,fragment,community,com/whoop/community,sources/CommunityFragment.java\n",
    );
    let mut input = input_with_rules();
    input.expected_inventory = Some(UiCoverageExpectedInventory {
        navigation_count: Some(2),
        layout_count: Some(2),
        ui_resource_count: Some(2),
        source_class_count: Some(2),
        navigation_destinations_sha256: Some(sha256_hex(
            &dir.path().join("navigation-destinations.csv"),
        )),
        layouts_sha256: Some(sha256_hex(&dir.path().join("layouts.csv"))),
        ui_resources_sha256: Some(sha256_hex(&dir.path().join("ui-resources.csv"))),
        source_ui_classes_sha256: Some(sha256_hex(&dir.path().join("source-ui-classes.csv"))),
    });

    let report = run_ui_coverage_audit(&input, dir.path()).unwrap();

    assert!(report.pass, "{:?}", report.issues);
    assert_eq!(report.inventory.navigation_count, 2);
    assert_eq!(
        report.inventory.navigation_destinations_sha256,
        input
            .expected_inventory
            .as_ref()
            .unwrap()
            .navigation_destinations_sha256
            .as_ref()
            .unwrap()
            .as_str()
    );
}

#[test]
fn expected_inventory_drift_fails_with_regeneration_next_action() {
    let dir = write_inventory(
        "graph,type,id,label,class_or_graph,deep_link_count,deep_links,argument_count,arguments,path\n\
         home_graph,fragment,home_fragment,Home,com.whoop.home.view.HomeFragment,0,,0,,res/navigation/home.xml\n\
         community_graph,fragment,communityFragment,CommunityFragment,com.whoop.community.CommunityFragment,0,,0,,res/navigation/community.xml\n",
        "resource,variant,category,root_tag,path\nfragment_home,layout,fragment,FrameLayout,res/layout/fragment_home.xml\nactivity_main,layout,activity,FrameLayout,res/layout/activity_main.xml\n",
        "class_name,category,module,package_path,path\nHomeFragment,fragment,home,com/whoop/home,sources/HomeFragment.java\nCommunityFragment,fragment,community,com/whoop/community,sources/CommunityFragment.java\n",
    );
    let mut input = input_with_rules();
    input.expected_inventory = Some(UiCoverageExpectedInventory {
        navigation_count: Some(1),
        layout_count: Some(2),
        ui_resource_count: Some(1),
        source_class_count: Some(2),
        navigation_destinations_sha256: Some("deadbeef".to_string()),
        layouts_sha256: Some(sha256_hex(&dir.path().join("layouts.csv"))),
        ui_resources_sha256: Some("badcafe".to_string()),
        source_ui_classes_sha256: Some(sha256_hex(&dir.path().join("source-ui-classes.csv"))),
    });

    let report = run_ui_coverage_audit(&input, dir.path()).unwrap();

    assert!(!report.pass);
    assert!(!report.inventory_valid);
    assert!(report.coverage_map_valid);
    assert!(report.all_surfaces_classified);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| { issue == "inventory_count_changed:navigation:expected_1:actual_2" })
    );
    assert!(report.issues.iter().any(|issue| {
        issue.starts_with("inventory_checksum_changed:navigation_destinations_csv:")
    }));
    assert!(
        report
            .issues
            .iter()
            .any(|issue| { issue == "inventory_count_changed:ui_resource:expected_1:actual_2" })
    );
    assert!(
        report
            .issues
            .iter()
            .any(|issue| { issue.starts_with("inventory_checksum_changed:ui_resources_csv:") })
    );
    assert!(report.next_actions.iter().any(|action| {
        action.reason == "inventory_count_changed"
            && action
                .action
                .contains("Review the regenerated APK UI inventory diff")
    }));
    assert!(report.next_actions.iter().any(|action| {
        action.reason == "inventory_checksum_changed"
            && action.action.contains("refresh expected_inventory")
    }));
}

fn input_with_rules() -> UiCoverageAuditInput {
    UiCoverageAuditInput {
        schema: "bull.ui-coverage-audit.v1".to_string(),
        inventory: UiCoverageInventoryPaths {
            navigation_destinations_csv: "navigation-destinations.csv".to_string(),
            layouts_csv: "layouts.csv".to_string(),
            ui_resources_csv: "ui-resources.csv".to_string(),
            source_ui_classes_csv: "source-ui-classes.csv".to_string(),
        },
        expected_inventory: None,
        coverage: UiCoverageRules {
            navigation: vec![
                NavigationCoverageRule {
                    rule_id: "nav-home-implement".to_string(),
                    graph: Some("home_graph".to_string()),
                    destination_id: Some("home_fragment".to_string()),
                    destination_type: None,
                    class_or_graph: None,
                    class_prefix: None,
                    status: UiCoverageStatus::Implement,
                    bull_area: "Today".to_string(),
                    target_level: Some("L3".to_string()),
                    reason: None,
                },
                NavigationCoverageRule {
                    rule_id: "nav-community-omit".to_string(),
                    graph: Some("community_graph".to_string()),
                    destination_id: None,
                    destination_type: None,
                    class_or_graph: None,
                    class_prefix: None,
                    status: UiCoverageStatus::Omit,
                    bull_area: "Out of scope".to_string(),
                    target_level: None,
                    reason: Some(
                        "Social/community features are not part of Bull MVP.".to_string(),
                    ),
                },
            ],
            layouts: vec![
                LayoutCoverageRule {
                    rule_id: "layout-fragment".to_string(),
                    resource: None,
                    category: Some("fragment".to_string()),
                    status: UiCoverageStatus::ApproximateLocally,
                    bull_area: "Screens".to_string(),
                    target_level: Some("component_inventory".to_string()),
                    reason: None,
                },
                LayoutCoverageRule {
                    rule_id: "layout-activity".to_string(),
                    resource: None,
                    category: Some("activity".to_string()),
                    status: UiCoverageStatus::ApproximateLocally,
                    bull_area: "Navigation shell".to_string(),
                    target_level: Some("component_inventory".to_string()),
                    reason: None,
                },
            ],
            ui_resources: vec![UiResourceCoverageRule {
                rule_id: "ui-resource-layout".to_string(),
                resource_type: Some("layout".to_string()),
                variant: None,
                resource: None,
                status: UiCoverageStatus::ApproximateLocally,
                bull_area: "Resource inventory".to_string(),
                target_level: Some("resource_inventory".to_string()),
                reason: None,
            }],
            source_classes: vec![SourceClassCoverageRule {
                rule_id: "source-fragment".to_string(),
                class_name: None,
                category: Some("fragment".to_string()),
                module: None,
                status: UiCoverageStatus::ApproximateLocally,
                bull_area: "Screen inventory".to_string(),
                target_level: Some("component_inventory".to_string()),
                reason: None,
            }],
        },
    }
}

fn write_inventory(navigation: &str, layouts: &str, source_classes: &str) -> tempfile::TempDir {
    write_inventory_with_resources(
        navigation,
        layouts,
        "type,variant,resource,path,bytes\nlayout,layout,fragment_home,res/layout/fragment_home.xml,128\nlayout,layout,activity_main,res/layout/activity_main.xml,256\n",
        source_classes,
    )
}

fn write_inventory_with_resources(
    navigation: &str,
    layouts: &str,
    ui_resources: &str,
    source_classes: &str,
) -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("navigation-destinations.csv"), navigation).unwrap();
    fs::write(dir.path().join("layouts.csv"), layouts).unwrap();
    fs::write(dir.path().join("ui-resources.csv"), ui_resources).unwrap();
    fs::write(dir.path().join("source-ui-classes.csv"), source_classes).unwrap();
    dir
}

fn sha256_hex(path: &Path) -> String {
    let bytes = fs::read(path).unwrap();
    format!("{:x}", Sha256::digest(bytes))
}
