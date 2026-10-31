# Spec Delta

## ADDED Requirements

### Requirement: Installer skips an installed version

`install.sh` SHALL download nothing when the `gsttt` in the install directory prints `gsttt <version>` for `--version`, `<version>` being the version it would install. It SHALL say that the version is already installed, print the `PATH` hint as after an install, and exit 0.

#### Scenario: An installed version is not downloaded again
- **WHEN** the `gsttt` in `GSTTT_INSTALL_DIR` prints `gsttt 0.1.1` and `install.sh` is to install 0.1.1
- **THEN** it says that 0.1.1 is already installed, fetches the latest version but no package, leaves the install directory as it was, and exits 0
