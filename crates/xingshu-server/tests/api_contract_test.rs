#[test]
fn openapi_contract_lists_runtime_routes_and_camel_case_fields() {
    let contract = include_str!("../../../docs/api/openapi.yaml");
    for route in [
        "/health:",
        "/api/v1/repos:",
        "/api/v1/repos/{repoId}:",
        "/api/v1/repos/{repoId}/pull:",
        "/api/v1/repos/{repoId}/tags:",
        "/api/v1/roots:",
        "/api/v1/roots/{rootId}:",
        "/api/v1/tags:",
        "/api/v1/tags/{tagId}:",
        "/api/v1/scan:",
        "/api/v1/stats:",
    ] {
        assert!(contract.contains(route), "missing route {route}");
    }
    for field in ["requestId", "repoId", "repoKind", "sizeBytes", "createdAt"] {
        assert!(contract.contains(field), "missing camelCase field {field}");
    }
    assert!(contract.contains("application/problem+json"));
}
