# SolidJS Frontend Implementation Plan

## Overview
This plan outlines the approach for implementing a SolidJS-based frontend for the Datum CLI application. The frontend will provide a browser-based interface while maintaining the local-first principles of the application.

## Architecture

### Directory Structure
```
/frontend
├── public/
│   ├── index.html               # Entry HTML file
│   └── favicon.ico              # Site favicon
├── src/
│   ├── App.tsx                  # Main application component
│   ├── index.tsx                # Entry point
│   ├── components/              # Reusable UI components
│   │   ├── Layout/              # Layout components
│   │   │   ├── AppLayout.tsx    # Main application layout
│   │   │   └── Panel.tsx        # Reusable panel component
│   │   ├── Terminal/            # Terminal-like interface
│   │   │   ├── Terminal.tsx     # Terminal container component
│   │   │   ├── TerminalInput.tsx # Command input component
│   │   │   └── TerminalOutput.tsx # Command output display
│   │   ├── DayView/             # 4-day dayview panel
│   │   │   ├── DayView.tsx      # DayView container
│   │   │   └── SVGRenderer.tsx  # Component to render SVG output
│   │   ├── NowView/             # Nowview panel
│   │   │   ├── NowView.tsx      # NowView container
│   │   │   └── SVGRenderer.tsx  # Component to render SVG output
│   │   ├── TailView/            # Tail view panel
│   │   │   └── TailView.tsx     # TailView container
│   │   └── SyncPopup/           # CouchDB sync popup
│   │       ├── SyncPopup.tsx    # Sync configuration modal
│   │       └── CredentialForm.tsx # Form for entering sync credentials
│   ├── hooks/                   # Custom hooks
│   │   ├── useTerminal.ts       # Terminal state and command processing
│   │   ├── useCommandHistory.ts # Command history management
│   │   ├── useDayView.ts        # DayView state and rendering
│   │   ├── useNowView.ts        # NowView state and rendering
│   │   ├── useTailView.ts       # TailView state and rendering
│   │   └── useDb.ts             # Database connection and management
│   ├── utils/                   # Utility functions
│   │   ├── commandRunner.ts     # Runs commands via datum()
│   │   ├── svgUtils.ts          # Utilities for SVG handling
│   │   ├── credentialStorage.ts # Secure credential storage
│   │   └── dbUtils.ts           # Database utilities
│   ├── context/                 # React context providers
│   │   ├── DbContext.tsx        # Database context provider
│   │   └── CommandContext.tsx   # Command processing context
│   └── styles/                  # Styling
│       ├── global.css           # Global styles
│       ├── layout.css           # Layout styles
│       └── themes.css           # Theme definitions
├── vite.config.ts               # Vite configuration
├── tsconfig.json                # TypeScript configuration
└── package.json                 # Frontend package configuration
```

### Integration with Existing CLI

To integrate with the existing CLI codebase, we'll:

1. **Command Integration**: Directly call `datum()` from mainArgs.ts to handle command processing
2. **Database Connection**: Use the existing connectDbBrowser.ts implementation for browser contexts
3. **Rendering**: Render SVG outputs directly in the browser (with potential for direct D3 integration in the future)

## Core Code Changes Required

Before implementing the frontend, several changes are needed in the core codebase to make it browser-compatible and suitable for integration. The following changes should be implemented as a series of focused commits.

### Implementation Guide for Core Changes

This section provides a detailed commit-by-commit breakdown for implementing the necessary core changes. Each commit should focus on a small, self-contained change to make review and possible rollbacks easier.

#### Commit 1: Create Output Interface

**Purpose:** Create a foundation for browser-compatible output handling

**Changes:**
1. Create a new interface in `/src/output/outputInterface.ts` that defines methods for different output types
2. Implement a console output provider that matches the current behavior
3. Add infrastructure for a browser output provider

**Files to modify:**
- Create new file: `/src/output/outputInterface.ts`
- Create new file: `/src/output/consoleOutput.ts`
- Create new file: `/src/output/browserOutput.ts` (skeleton)

#### Commit 2: Refactor Output Main Functions

**Purpose:** Modify core output functions to use the interface

**Changes:**
1. Refactor `showSingle()`, `showHeaderLine()`, and `showMainInfoLine()` in output.ts
2. Make these functions return formatted strings and accept an output provider
3. Ensure backward compatibility with console output

**Files to modify:**
- `/src/output/output.ts`

#### Commit 3: Refactor Secondary Output Functions

**Purpose:** Complete the output system refactoring

**Changes:**
1. Refactor remaining output functions (`showCreate()`, `showExists()`, etc.)
2. Update all functions to use the new output interface
3. Add unit tests for the new output system

**Files to modify:**
- `/src/output/output.ts`
- Create/update test files as needed

#### Commit 4: Update DayView Command

**Purpose:** Make dayview command browser-compatible

**Changes:**
1. Modify `dayviewCmd.ts` to use the output interface
2. Ensure proper SVG string return
3. Remove direct console.log calls

**Files to modify:**
- `/src/commands/dayviewCmd.ts`

#### Commit 5: Update NowView Command

**Purpose:** Make nowview command browser-compatible

**Changes:**
1. Modify `nowviewCmd.ts` to use the output interface
2. Ensure proper SVG string return
3. Remove direct console.log calls

**Files to modify:**
- `/src/commands/nowviewCmd.ts`

#### Commit 6: Update Tail Command

**Purpose:** Make tail command browser-compatible

**Changes:**
1. Modify `tailCmd.ts` to use the output interface
2. Return formatted output strings instead of using console.log
3. Support both browser and console modes

**Files to modify:**
- `/src/commands/tailCmd.ts`

#### Commit 7: Improve Error Handling

**Purpose:** Replace process.exit() with structured errors

**Changes:**
1. Create structured error classes in a new file
2. Replace process.exit() calls with proper error throwing
3. Ensure errors can be displayed appropriately in browser context

**Files to modify:**
- Create new file: `/src/errors/browserCompatibleErrors.ts`
- `/src/index.ts`
- `/src/input/mainArgs.ts`

#### Commit 8: Update Main Command Interface

**Purpose:** Make datum() function browser-compatible

**Changes:**
1. Modify `datum()` function to return structured output
2. Add support for different output types (console/browser)
3. Create a browser-specific version of the function if needed

**Files to modify:**
- `/src/input/mainArgs.ts`

#### Commit 9: Browser-Compatible Database Connection

**Purpose:** Ensure database connection works in browser environment

**Changes:**
1. Update `connectDbBrowser.ts` to ensure all dependencies are browser-compatible
2. Add credential storage and sync configuration support
3. Add reconnection logic for browser environment

**Files to modify:**
- `/src/auth/connectDbBrowser.ts`
- Create new file: `/src/auth/credentialStorage.ts`

#### Commit 10: Integration Tests for Browser Compatibility

**Purpose:** Verify the changes work correctly in a browser-like environment

**Changes:**
1. Add integration tests for the browser-compatible interfaces
2. Test output formatting, error handling, and command execution
3. Mock browser environment for testing

**Files to modify:**
- Create new test files as needed

### Detailed Changes Description

#### 1. Output Handling Refactoring

- `/src/output/outputInterface.ts`:
  - Define interfaces for different output types (text, structured, SVG)
  - Create methods for different output scenarios (info, error, warning)
  - Support both console and browser outputs

- `/src/output/output.ts`:
  - Refactor functions to return strings/structured data instead of using console.log
  - Modify `showSingle()`, `showCreate()`, `showHeaderLine()`, `showMainInfoLine()` to return formatted strings
  - Add output provider parameter to functions

- `/src/commands/dayviewCmd.ts` and `/src/commands/nowviewCmd.ts`:
  - Ensure SVG output is properly returned as a string (already partially implemented)
  - Remove direct console.log calls and use the output interface

- `/src/commands/tailCmd.ts`:
  - Refactor to return formatted output through the output interface

#### 2. Error Handling Improvements

- `/src/errors/browserCompatibleErrors.ts`:
  - Create structured error classes for different error types
  - Include additional metadata needed for display in browser

- `/src/index.ts` and `/src/input/mainArgs.ts`:
  - Replace process.exit() calls with proper error throwing
  - Use the new error classes for structured error handling

#### 3. Command Interface Modifications

- `/src/input/mainArgs.ts`:
  - Update the `datum()` function to return structured output
  - Add parameters for output providers
  - Create helper functions for browser-specific handling

#### 4. Database Connection Updates

- `/src/auth/connectDbBrowser.ts`:
  - Verify browser compatibility of dependencies
  - Update imports as needed

- `/src/auth/credentialStorage.ts`:
  - Implement secure credential storage
  - Add sync configuration handling

## Frontend Implementation Strategy

After the core code changes are completed, the frontend implementation should also follow a systematic, commit-by-commit approach:

### Commit Strategy for Frontend Implementation

Each phase of the frontend implementation should be broken down into focused commits. Aim for commits that:
1. Implement a single, well-defined feature or component
2. Can be tested independently
3. Don't break existing functionality
4. Include appropriate tests

### Phase 1: Setup and Basic Structure (Commits 1-4)

#### Commit 1: Initialize Frontend Project
- Set up the SolidJS project structure with Vite
- Add required dependencies to the frontend package.json
- Configure TypeScript settings for compatibility with the main project
- Create basic build scripts

#### Commit 2: Configure Build System
- Set up build system for bundling the datum core code
- Configure GitHub Pages deployment workflow
- Add basic project documentation

#### Commit 3: Create Application Shell
- Implement the base application layout
- Create container components for the main panels
- Set up the basic CSS structure
- Implement theme variables and basic styling

#### Commit 4: Set up Core Integrations
- Set up initial imports from the core code
- Configure module aliases
- Add basic tests for integration with core code

### Phase 2: Core Functionality (Commits 5-8)

#### Commit 5: Database Connection
- Implement PouchDB connection using the existing connectDbBrowser functionality
- Set up database context provider for application-wide access
- Add connection status indicators
- Implement basic error handling for database operations

#### Commit 6: Terminal Interface - Input
- Create the terminal input component with command history support
- Implement keyboard navigation (up/down arrows for history)
- Add input focus management
- Style the terminal input to match the wireframe

#### Commit 7: Terminal Interface - Output
- Implement terminal output display component
- Add support for different output types (text, structured, SVG)
- Implement inline error handling in the terminal output
- Style the terminal output to match the wireframe

#### Commit 8: Command Processing
- Implement command processing by directly calling `datum()` from mainArgs.ts
- Create command context provider
- Add command execution feedback
- Implement error handling for command execution

### Phase 3: View Components (Commits 9-12)

#### Commit 9: SVG Renderer Component
- Create reusable SVG renderer component
- Add support for rendering SVG strings
- Implement proper SVG sizing and responsiveness
- Add error handling for SVG rendering issues

#### Commit 10: DayView Panel
- Implement the 4-day dayview panel component
- Integrate with dayviewCmd to get SVG output
- Add refresh/reload functionality
- Style to match the wireframe design

#### Commit 11: NowView Panel
- Implement the nowview panel component
- Integrate with nowviewCmd to get SVG output
- Add auto-refresh functionality
- Style to match the wireframe design

#### Commit 12: TailView Component
- Implement the tail view component
- Integrate with tailCmd for content
- Add auto-update functionality
- Style to match the wireframe design
- Ensure proper scrolling and overflow behavior

### Phase 4: Integration and Refinement (Commits 13-16)

#### Commit 13: Layout Integration
- Integrate all panels into the final layout
- Ensure proper sizing and spacing between panels
- Implement responsive behavior for different screen sizes
- Add loading states for all components

#### Commit 14: CouchDB Sync Configuration
- Create the CouchDB sync configuration popup component
- Implement credential form with validation
- Add secure credential storage using the browser's storage APIs
- Add connection testing functionality

#### Commit 15: Automatic Reconnection
- Implement automatic reconnection to avoid re-entering credentials
- Add sync status indicators
- Implement graceful handling of connection failures
- Add offline mode detection and notification

#### Commit 16: Final Refinements
- Optimize for performance
- Add comprehensive error handling
- Implement loading states for all operations
- Add final styling touches
- Complete documentation

## Technical Considerations

### Browser Compatibility
- Ensure compatibility with modern browsers (Chrome, Firefox, Safari, Edge)
- Use progressive enhancement where possible

### State Management
- Use SolidJS's fine-grained reactivity for state management
- Create a central store for application state
- Use context providers for shared state

### Performance Optimization
- Implement virtualization for large datasets in views
- Use lazy loading for components when appropriate
- Optimize rendering cycles

### Styling Approach
- Use CSS modules for component-specific styling
- Implement a clean, dense UI with minimal spacing
- Use responsive design principles
- Follow existing application color schemes

## Integration Points

### Database Integration
- Use connectDb with pouchdb-browser to establish connections
- Implement sync functionality with remote CouchDB servers
- Handle offline/online states appropriately

### Command Integration
- Map CLI commands to browser-compatible functions
- Ensure proper error handling for browser context
- Create virtual console output for command results

## UI Wireframe

The following wireframe demonstrates the desired UI layout:

![SolidJS Frontend Wireframe](wireframe.png)

The interface should be clean but dense, with minimal gaps between the panels, filling almost the entire viewport.

## Clarified Requirements

Based on discussions with the project owner:

1. **CLI Integration**: The frontend should integrate directly with the CLI code by calling `datum()` from mainArgs.ts.

2. **Command Handling**: For commands requiring Node.js APIs not available in browsers, we'll initially ignore this constraint and focus on implementing the interface.

3. **Database Connection**: We'll use the existing connectDbBrowser.ts implementation.

4. **View Rendering**: SVG outputs from dayview/nowview will be rendered directly in the browser. In the future, direct D3 integration may be implemented for better handling of data changes.

5. **Browser Compatibility**: No specific requirements; targeting modern browsers is sufficient.

6. **Build System**: The frontend will have its own build system separate from the main CLI.

7. **Authentication**: For CouchDB synchronization, users will provide host, username, and password. These credentials should be securely stored to avoid re-entering upon refresh.

8. **Terminal Features**: The terminal interface should support command history (accessible with up/down arrow keys). Command auto-completion is not required for the initial implementation.

9. **Layout Configuration**: Initially, the layout will be fixed without user configuration options for resizing or showing/hiding panels.

10. **Error Handling**: Errors should be displayed inline in the terminal output, consistent with CLI behavior.

11. **Keyboard Shortcuts**: No specific keyboard shortcuts required for the initial implementation.

12. **Offline/Online Transitions**: PouchDB will handle offline/online transitions for syncing. The application should leverage this built-in capability.

13. **Performance Considerations**: No specific performance optimizations needed for large datasets in the initial implementation.

14. **Data Visualization**: Initial implementation will only use the SVG rendering without additional data visualization capabilities.

## Additional Clarifications

Based on further discussions with the project owner:

1. **Command Auto-completion**: Not required for the initial implementation, only command history support is needed.

2. **Offline-First Architecture**: PouchDB already handles the offline-first approach with local storage and remote sync, so no additional implementation is needed for this feature.

3. **Browser Compatibility**: Target modern browsers only; no need to support older browsers or implement complex compatibility solutions.

4. **Build System**: The frontend build system should be completely separate from the main project, but should bundle the datum core code appropriately.

5. **Accessibility**: No specific accessibility requirements for the initial implementation.

6. **Analytics/Telemetry**: No analytics or telemetry should be included in the frontend.

7. **Data Export**: No specific data export functionality is required in the initial implementation.

8. **Deployment**: The application should be easy to deploy to GitHub Pages, with appropriate build scripts and documentation.

## Testing Strategy

### Unit Tests
- Test individual components and hooks
- Mock command interfaces and database connections

### Integration Tests
- Test interactions between components
- Test command processing workflow

### End-to-End Tests
- Test full user journeys
- Test synchronization capabilities

## Build and Deployment Strategy (Commits 17-18)

### Commit 17: Build System Finalization
- Finalize the independent build system using Vite
- Optimize bundle sizes
- Configure production builds
- Ensure proper code splitting
- Set up comprehensive build scripts

**Build System Details:**
- Use Vite for fast development and optimized production builds
- Configure TypeScript settings to ensure compatibility with the main project
- Bundle the datum core code appropriately for browser use
- Implement tree-shaking for smaller bundle sizes
- Configure environment variables for different deployment environments

### Commit 18: Deployment Configuration
- Set up GitHub Pages deployment workflow
- Create deployment documentation
- Implement offline capabilities
- Add final tests for deployed application

**Deployment Details:**
- Configure static file serving for the built frontend
- Create GitHub Actions workflow for automated deployments
- Document deployment process in README.md
- Add a service worker for offline functionality
- Configure proper caching strategies

### Credential Storage Implementation Details
- Use browser's localStorage with encryption for storing credentials
- Implement the Web Crypto API for securing sensitive information
- Add credential validation
- Implement secure credential deletion
- Add timeout/expiration mechanism

### Offline Functionality Details
- Implement caching strategies for offline access
- Add sync queue for operations performed offline
- Provide clear UI indication of offline status
- Ensure seamless transition between online and offline modes