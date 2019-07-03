//! Module for prototyping things.

use crate::glfw_context::GlfwContext;
use crate::{backbone, RenderEvent, TickEvent, KeyEvent, MouseEvent, MouseMoveEvent};
use crate::common::resources;
use crate::blocks;
use crate::render;
use std::rc::Rc;

pub mod freecam;
use freecam::Freecam;

pub mod sky;
pub mod grid;
pub mod crosshair;

pub mod test_blocks;
use test_blocks::ChunkStorage;
use test_blocks::ChunkRenderManager;

pub fn setup(
	backbone: &mut backbone::Backbone,
	glfw_context: &mut GlfwContext,
	res: &mut resources::Resources,
) {
	let blocks = blocks::Blocks::new().to_ref();
	
	let chunks = ChunkStorage::new(&blocks);
	
	let chdraw = ChunkRenderManager::new(
		&glfw_context.gl,
		res,
		&blocks
	).map_err(|_| {
		error!("Failed to load 'Blocks' material.");
	}).unwrap();
	
	let camera = Freecam::new();
	
	let sky = sky::SkyRenderer::new(&glfw_context.gl, res).map_err(|_| {
		error!("Failed to load 'Blocks' material.");
	}).unwrap();
	
	let grid = grid::GridRenderer::new(&glfw_context.gl, res).map_err(|_| {
		error!("Failed to load 'Grid' material.");
	}).unwrap();
	
	let solid_color_material = render::materials::SolidColorMaterial::new(&glfw_context.gl, &res).map_err(|_| {
		error!("Failed to load 'Grid' material.");
	}).unwrap();
	let solid_color_material = Rc::new(solid_color_material);
	
	let crosshair_2d = crosshair::CrosshairRenderer2D::new(&glfw_context.gl, &solid_color_material);
	let crosshair_3d = crosshair::CrosshairRenderer3D::new(&glfw_context.gl, &solid_color_material);
	
	let playground = Playground {
		blocks,
		chunks,
		chdraw,
		camera,
		sky,
		grid,
		crosshair_2d,
		crosshair_3d,
	};
	
	let playground = Box::new(playground);
	let _playground_id = backbone.node_new(
		backbone.root_get_id(),
		"playground",
		Some(playground)
	).unwrap();
	
	// TODO: Attach more components.
}

pub struct Playground {
	blocks: Rc<blocks::Blocks>,
	chunks: ChunkStorage,
	chdraw: ChunkRenderManager,
	camera: Freecam,
	sky: sky::SkyRenderer,
	grid: grid::GridRenderer,
	crosshair_2d: crosshair::CrosshairRenderer2D,
	crosshair_3d: crosshair::CrosshairRenderer3D,
}

impl backbone::Handler for Playground {
	fn on_event<'a>(&mut self, event: &mut backbone::Wrapper, context: &mut backbone::Context) {
		let phase = event.get_phase().clone();
		
		if let Some(mouse_move_event) = event.downcast::<MouseMoveEvent>() {
			self.camera.update_rotation(
				mouse_move_event.dx as f32,
				mouse_move_event.dy as f32
			);
			
			let mut rc = self.camera.get_block_raytrace(16.0, 1.0);
			if let Some((_, curr, _)) = self.chunks.raycast(&mut rc) {
				self.camera.target = Some(curr);
			} else {
				self.camera.target = None;
			}
			
			return
		}
		
		if let Some(mouse_event) = event.downcast::<MouseEvent>() {
			match mouse_event {
				MouseEvent{button, action: glfw::Action::Press, modifiers: _} => {
					if ! self.camera.active {
						return;
					}
					
					let air = self.blocks
						.get_block_by_name_unchecked("air")
						.get_default_state();
					
					let bedrock = self.blocks
						.get_block_by_name_unchecked("adm")
						.get_default_state();
					
					let used_block = self.camera.block.unwrap_or(bedrock);
					
					let mut rc = self.camera.get_block_raytrace(16.0, 1.0);
					
					if let Some((last, curr, _)) = self.chunks.raycast(&mut rc) {
						let t = match button {
							glfw::MouseButtonLeft => {
								Some((&curr, air))
							},
							glfw::MouseButtonRight => {
								Some((&last, used_block))
							},
							_ => None
						};
						
						if let Some((pos, block)) = t {
							self.chunks.set_block(&pos, block);
						}
					}
					
				},
				_ => (),
			};
			
			event.stop();
			return
		}
		
		if let Some(key_event) = event.downcast::<KeyEvent>() {
			match key_event {
				KeyEvent{key: glfw::Key::C, scancode: _, action: glfw::Action::Press, modifiers: _} => {
					self.camera.crane = !self.camera.crane;
				},
				_ => (),
			}
			
			return
		}
		
		if let Some(tick) = event.downcast::<TickEvent>() {
			let glfw_context = context
				.component_get_mut::<GlfwContext>().ok().unwrap();
			
			self.camera.active = glfw_context.window.get_cursor_mode() == glfw::CursorMode::Disabled;
			self.camera.update_movement(&glfw_context.window, 1.0 / tick.tps as f32);
			
			let mut rc = self.camera.get_block_raytrace(16.0, 1.0);
			if let Some((_, curr, _)) = self.chunks.raycast(&mut rc) {
				self.camera.target = Some(curr);
			} else {
				self.camera.target = None;
			}
			return
		}
		
		if let Some(render_event) = event.downcast::<RenderEvent>() {
			
			if let backbone::Phase::Action = phase {
				self.render_scene(render_event);
			}
			
			if let backbone::Phase::Bubbling = phase {
				let text_renderer = context
					.component_get_mut::<render::text::TextRendererComp>().ok().unwrap();
				
				self.render_hud(render_event, text_renderer);
			}
			
			return
		}
		
		debug!("Playground received {} in {}-phase.", event.event.get_type_name(), event.get_phase());
	}
}

impl Playground {
	
	pub fn render_scene(&mut self, revt: &RenderEvent) {
		use crate::render::*;
		
		let proj_matrix = self.camera.get_gl_projection_matrix((revt.width, revt.height), revt.interpolation);
		let skyw_matrix = self.camera.get_gl_view_matrix(false, revt.interpolation);
		let view_matrix = self.camera.get_gl_view_matrix(true, revt.interpolation);
		let transform = proj_matrix * view_matrix;
		
		self.sky.render(
			&proj_matrix,
			&skyw_matrix,
			&self.camera.get_position(revt.interpolation)
		);
		
		self.grid.render(
			&transform,
			&self.camera.get_position(revt.interpolation),
		);
		
		unsafe {
			revt.gl.Enable(gl::DEPTH_TEST);
		}
		
		self.chdraw.render(&self.chunks, &transform);
		
		if let Some(target) = &self.camera.target {
			self.crosshair_3d.draw(&transform, target)
		}
	}
	
	pub fn render_hud(&mut self, revt: &RenderEvent, text: &mut render::text::TextRendererComp) {
		
		
		unsafe {
			revt.gl.Disable(gl::DEPTH_TEST);
			revt.gl.Enable(gl::BLEND);
			revt.gl.BlendFunc(gl::ONE, gl::ONE_MINUS_SRC_ALPHA);
		}
		
		let projection = cgmath::Matrix4::from(cgmath::ortho(
			0.0, revt.width as f32,
			revt.height as f32, 0.0,
			-1.0, 1.0
		));
		
		self.crosshair_2d.draw(&projection, revt.width, revt.height, 4.0);
		
		text.transform = projection;
		text.draw_text(&format!("Blocks: {}", self.chunks.get_approximate_volume()), 16.0, 1.0, 2.0);
	}
	
}