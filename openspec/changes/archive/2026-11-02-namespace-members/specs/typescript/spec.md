# Spec Delta

## ADDED Requirements

### Requirement: A qualified type name and a JSX member are used by their name

The connector SHALL record a use of `lib.Foo` for the qualified type name `lib.Foo`, for the type query `typeof lib.Foo` and for the tag `<lib.Foo />`, as it does for the value `lib.Foo`, and not a use of the bare root `lib`. A root that is no binding records nothing.

#### Scenario: A qualified type name is used by its whole name

- **WHEN** a function writes `lib.Foo`, `Array<lib.Foo>` or `typeof lib.helper` in a type position, where `lib` is a namespace import
- **THEN** the function uses `lib.Foo`, or `lib.helper`, and not `lib`
- **AND** a value typed `lib.Foo` that is returned or passed on uses `lib.Foo` and not `lib`
- **AND** a qualified name whose root is no binding is no use

#### Scenario: A JSX member is used by its whole name

- **WHEN** a function renders `<lib.Comp />`, where `lib` is a namespace import
- **THEN** the function uses `lib.Comp` and not `lib`
