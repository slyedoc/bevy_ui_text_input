//! minimal text input example

use bevy::{color::palettes::css::NAVY, input_focus::InputFocus, prelude::*, ui_widgets::observe};
use bevy_ui_text_input::{
    SubmitText, TextInputFilter, TextInputMode, TextInputNode, TextInputPlugin,
};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, TextInputPlugin))
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands, mut active_input: ResMut<InputFocus>) {
    // UI camera
    commands.spawn(Camera2d);

    let input_entity = commands
        .spawn((
            TextInputNode {
                mode: TextInputMode::SingleLine,
                max_chars: Some(10),
                ..Default::default()
            },
            TextInputFilter::Decimal,
            TextFont {
                font_size: 20.,
                ..Default::default()
            },
            Node {
                width: Val::Px(100.),
                height: Val::Px(20.),
                ..default()
            },
            BackgroundColor(NAVY.into()),
            observe(|event: On<SubmitText>| {
                let d: f64 = event.text.parse().unwrap();
                println!("decimal: {d}");   
            })
        ))
        .id();

    active_input.set(input_entity);

    commands
        .spawn(Node {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(10.),
            ..Default::default()
        })
        .add_child(input_entity);
}
