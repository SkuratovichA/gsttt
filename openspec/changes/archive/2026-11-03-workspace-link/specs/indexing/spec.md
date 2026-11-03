# Spec Delta

## ADDED Requirements

### Requirement: A link across projects
A project that asks to link across projects SHALL receive, besides its own files, the files of other projects of the same connector that its files import, directly or through the files they import. A project that does not ask SHALL receive its own files only. A file of another connector SHALL never be one of them.

#### Scenario: A project that links across projects sees the files its files import
- **WHEN** a project that links across projects has a file that imports a file of a second project, which imports a file of a third, and a fourth project's file is imported by nothing
- **THEN** its link sees the files of the second and the third project and not the fourth

#### Scenario: A project that does not link across projects sees only its own files
- **WHEN** a project that does not link across projects has a file that imports a file of another project
- **THEN** its link sees no file of the other project

#### Scenario: A file of another connector is never a neighbour
- **WHEN** a project that links across projects has a file that imports a file that another connector reads
- **THEN** its link does not see that file

#### Scenario: A neighbour whose link data does not read back is left out of the link
- **WHEN** a file of another project that a file imports has link data that does not read back
- **THEN** the link leaves that file out and gives its facts for the files it reads

### Requirement: Linked facts of several projects
A linked fact that replaces a key SHALL take away the linked fact of that key on the same file, line and role, unless it carries that key itself. A linked fact that several projects give for the same file, line, role, key and reader SHALL count once.

#### Scenario: A linked fact that replaces a key takes away another project's linked fact of that key
- **WHEN** one project links a fact on a line and another project links a fact on that line that replaces its key
- **THEN** only the second fact is a site

#### Scenario: Two projects giving the same linked fact give one
- **WHEN** two projects link the same fact on the same line of a file
- **THEN** the file has one site for it
