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

## Implementation Strategy

### Phase 1: Setup and Basic Structure
- Set up the SolidJS project structure with Vite
- Add required dependencies to the frontend package.json
- Create basic layout components and application shell
- Configure TypeScript settings for compatibility with the main project

### Phase 2: Core Functionality
- Implement PouchDB connection using the existing connectDbBrowser functionality
- Create the terminal-like interface for command input and output with command history support
- Implement inline error handling in the terminal output
- Implement command processing by directly calling `datum()` from mainArgs.ts

### Phase 3: View Components
- Implement the 4-day dayview panel by directly rendering SVG output from dayviewCmd
- Implement the nowview panel by directly rendering SVG output from nowviewCmd
- Implement the tail view component
- Ensure responsive layout and proper sizing with minimal gaps between panels

### Phase 4: Integration and Refinement
- Create the CouchDB sync configuration popup with secure credential storage
- Implement automatic reconnection to avoid re-entering credentials
- Optimize for performance
- Add proper error handling and loading states

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

![SolidJS Frontend Wireframe](frontend/docs/wireframe.png)

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

8. **Terminal Features**: The terminal interface should support command history (accessible with up/down arrow keys) and auto-completion similar to a typical shell.

9. **Layout Configuration**: Initially, the layout will be fixed without user configuration options for resizing or showing/hiding panels.

10. **Error Handling**: Errors should be displayed inline in the terminal output, consistent with CLI behavior.

11. **Keyboard Shortcuts**: No specific keyboard shortcuts required for the initial implementation.

12. **Offline/Online Transitions**: PouchDB will handle offline/online transitions for syncing. The application should leverage this built-in capability.

13. **Performance Considerations**: No specific performance optimizations needed for large datasets in the initial implementation.

14. **Data Visualization**: Initial implementation will only use the SVG rendering without additional data visualization capabilities.

## Remaining Question

1. Is there a specific visual style guide to follow beyond "clean but dense"?

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

## Build and Deployment Considerations

### Build System
- Setup an independent build system using Vite for the frontend
- Configure TypeScript settings to ensure compatibility with the main project
- Use module federation or similar techniques to share code with the main CLI

### Credential Storage
- Implement secure credential storage for CouchDB sync
- Use browser's localStorage or IndexedDB with encryption for storing credentials
- Consider using the Web Crypto API for securing sensitive information
- Implement auto-reconnect functionality to avoid re-entering credentials

### Deployment
- Configure static file serving for the built frontend
- Support various hosting environments
- Enable offline functionality