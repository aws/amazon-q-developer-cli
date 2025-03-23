# Amazon Q Developer CLI Worklog

This file tracks significant changes and improvements to the Amazon Q Developer CLI.

## 2025-03-23

### Trajectory Visualization Improvements

- Fixed issue with `--auto-visualize` flag by automatically opening the visualization in the default browser when generated
- Added the `open` crate as a dependency to handle opening files in the browser
- Modified the `generate_visualization` method in `TrajectoryRecorder` to open the visualization after generating it

This enhancement improves the user experience by eliminating the need to manually locate and open the visualization file after it's generated.
