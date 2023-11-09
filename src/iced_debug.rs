use iced_wgpu::Renderer;
use iced_widget::{slider, text_input, Column, Row, Text};
use iced_winit::core::{Alignment, Color, Element, Length};
use iced_winit::runtime::{Command, Program};
use iced_winit::style::Theme;

pub struct DebugControls {
}

#[derive(Debug, Clone)]
pub enum DebugControlsMessage {
}

impl DebugControls {
    pub fn new() -> DebugControls {
        DebugControls {
        }
    }
}

const RED:Color = Color { r: 1.0,  g: 0.0,  b: 0.0,  a: 1.0 };

impl Program for DebugControls {
    type Renderer = Renderer<Theme>;
    type Message = DebugControlsMessage;

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        Command::none()
    }

    fn view(&self) -> Element<Self::Message, Renderer<Theme>> {
        Row::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .align_items(Alignment::End)
            .push(
                Column::new()
                    .width(Length::Fill)
                    .align_items(Alignment::End)
                    .push(
                        Text::new("TESTING DEBUG UI")
                            .style(RED),
                    )
            )
            .into()
    }
}