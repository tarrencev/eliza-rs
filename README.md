# Asuka

Asuka is a powerful agent framework seamlessly integrated with the Dojo stack, empowering developers to bring their games and applications to life with intelligent, autonomous agents. Whether you need NPCs that feel truly alive, dynamic storytellers that craft compelling narratives, or natural language interfaces that let players interact with your world in intuitive ways - Asuka makes it possible.

Asuka enables your agents to:

-   Act as believable NPCs with distinct personalities and behaviors
-   Generate dynamic storylines and quests that adapt to player actions
-   Provide natural language interfaces for intuitive world interaction
-   Participate in games and challenges with human-like reasoning
-   Create emergent gameplay through autonomous decision making

## Project Structure

The project is organized into several key components:

-   `asuka-core`: Core functionality for the conversational agent
-   `asuka-starknet`: Starknet integration components
-   `examples`: Example implementations and usage patterns

## Getting Started

1.  Ensure you have Rust installed
2.  Clone the repository
3.  Set up your environment variables (copy `.env.example` to `.env` if provided)
4.  Build the project:
    ```bash
    cargo build
    ```

## Examples

Check the `examples` directory for implementation examples and usage patterns.

## Development

This project uses a workspace structure with multiple crates:

-   Main workspace members are defined in `Cargo.toml`
-   Each crate can be built and tested independently
-   The project uses Cargo workspace for dependency management

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
