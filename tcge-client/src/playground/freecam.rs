use glfw::{Key, Action};
use cgmath::Matrix4;
use cgmath::Vector3;
use cgmath::Transform;
use cgmath::InnerSpace;
use super::aabb::AxisAlignedBoundingBox;

pub struct Freecam {
	pub active: bool,
	position: cgmath::Vector3<f32>,
	velocity: cgmath::Vector3<f32>,
	rotation: cgmath::Vector2<f32>,
	position_last: cgmath::Vector3<f32>,
	velocity_last: cgmath::Vector3<f32>,
	rotation_last: cgmath::Vector2<f32>,
	pub target: Option<blocks::BlockCoord>,
	min_depth: f32,
	max_depth: f32,
	field_of_view: f32,
	fov_vel_effect: bool,
	mouse_sensivity: f32,
	invert_mouse: bool,
	move_speed: f32,
	pub crane: bool,
	pub gravity: bool,
}

impl Freecam {
	/// Creates a new camera with default settings.
	pub fn new() -> Self {
		return Self {
			active: true,
			position: cgmath::Vector3 { x: 0.0, y: 1.8, z: -3.0 },
			velocity: cgmath::Vector3 { x: 0.0, y: 0.0, z: 0.0 },
			rotation: cgmath::Vector2 { x: 0.0, y: 0.0 },
			position_last: cgmath::Vector3 { x: 0.0, y: 1.8, z: 0.0 },
			velocity_last: cgmath::Vector3 { x: 0.0, y: 0.0, z: 0.0 },
			rotation_last: cgmath::Vector2 { x: 0.0, y: 90.0 },
			target: None,
			min_depth: 0.1,
			max_depth: 4096.0,
			field_of_view: 90.0,
			fov_vel_effect: false,
			mouse_sensivity: 0.0625,
			invert_mouse: false,
			move_speed: 0.5,
			crane: true,
			gravity: false
		}
	}
	
	/// Returns the predicted position of the camera for a given interpolation factor.
	/// Pass in `0` to get the current position as updated in the last tick.
	pub fn get_position(&self, interpolation: f32) -> cgmath::Vector3<f32> {
		// simple movement prediction formula
		self.position + (self.velocity * interpolation)
	}
	
	pub fn get_velocity(&self, interpolation: f32) -> cgmath::Vector3<f32> {
		// simple movement prediction formula
		self.velocity * interpolation
	}
	
	/// Returns the predicted rotation of the camera for a given interpolation factor.
	/// Pass in `0` to get the current rotation as updated in the last tick.
	pub fn get_rotation_euler(&self, _interpolation: f32) -> cgmath::Vector2<f32> {
		// TODO: movement prediction
		self.rotation // + ((self.rotation_last - self.rotation) * interpolation)
	}
	
	pub fn get_look_dir(&self, interpolation: f32) -> cgmath::Vector3<f32> {
		let rotation = self.get_rotation_euler(interpolation);
		let pitch = cgmath::Deg(rotation.x);
		let yaw = cgmath::Deg(rotation.y);
		
		let mut camera = Matrix4::one();
		camera = camera * Matrix4::from_angle_y(yaw);
		camera = camera * Matrix4::from_angle_x(pitch);
		
		let forward = Vector3::new(0.0, 0.0, 1.0);
		let forward = Matrix4::transform_vector(&camera, forward);
		forward.normalize()
	}
	
	pub fn get_block_raytrace(&self, len: f32, interpolation: f32) -> blocks::BlockRaycast {
		let src = self.get_position(interpolation);
		let src = (src.x, src.y, src.z);
		
		let dir = self.get_look_dir(interpolation);
		let dir = (dir.x, dir.y, dir.z);
		
		blocks::BlockRaycast::new_from_src_dir_len(src, dir, len)
	}
	
	/// Updates the camera rotation by adding the given pitch/yaw euler-deltas.
	pub fn update_rotation(&mut self, yaw: f32, pitch: f32) -> bool {
		self.rotation_last.clone_from(&self.rotation);
		
		if !self.active {
			return false;
		}
		
		let pitch = if self.invert_mouse { -pitch } else { pitch };
		
		self.rotation.x += pitch * self.mouse_sensivity;
		self.rotation.x = clamp(self.rotation.x, -90.0, 90.0);
		
		self.rotation.y += yaw * self.mouse_sensivity;
		self.rotation.y = wrap(self.rotation.y, 360.0);
		true
	}
	
	/// Updates the camera position by querying key-states and changing the velocity accordingly.
	pub fn update_movement(&mut self, window: &glfw::Window, delta: f32, chunks: &super::ChunkStorage) {
		self.position_last.clone_from(&self.position);
		self.velocity_last.clone_from(&self.velocity);
		
		if !self.active {
			return;
		}
		
		let mut move_speed = self.move_speed * delta;
		
		// --- Apply speed multiplier?
		if window.get_key(Key::LeftShift) == Action::Press {
			move_speed *= 5.0;
		}
		
		// --- Construct velocity vector...
		let yaw = cgmath::Deg(self.rotation.y);
		let mut mat = Matrix4::from_angle_y(yaw);
		
		// Fetch the input statuses and convert them to 0/1...
		let forwards = (window.get_key(Key::W) == Action::Press) as i8;
		let backwards = (window.get_key(Key::S) == Action::Press) as i8;
		let strafe_left = (window.get_key(Key::A) == Action::Press) as i8;
		let strafe_right = (window.get_key(Key::D) == Action::Press) as i8;
		
		let mut direction = Vector3::new(0.0, 0.0, 0.0);
		
		// ...then build a direction vector from them.
		// - If neither are active, the result is 0.
		// - If only 'forwards'  is active, the result is +1.
		// - If only 'backwards' is active, the result is -1.
		// - If both are active, cancelling each other out, the result is 0.
		direction.z += (forwards - backwards) as f32;
		direction.x += (strafe_right - strafe_left) as f32;
		
		// crane or drone mode for y axis
		if self.crane {
			// CRANE: The camera pitch does not affect planar movement.
			let up = (window.get_key(Key::Space) == Action::Press) as i8;
			let down = (window.get_key(Key::LeftControl) == Action::Press) as i8;
			direction.y += (up - down) as f32;
		}
		else {
			// DRONE: The camera pitch tilts the plane of movement.
			let pitch = cgmath::Deg(self.rotation.x);
			mat = mat * Matrix4::from_angle_x(pitch);
		}
		
		// Ensure that the vector has a magnitude of 1 (equal in all directions)
		direction.normalize();
		
		// Transform the new velocity vector into world-space...
		let direction = Matrix4::transform_vector(&mat, direction);
		
		// ...and add it to the existing velocity vector.
		self.velocity += direction * move_speed;
		
		let gravity_reduce: f32 = 9.81 * delta;
		let gravity_decell: f32 = 0.35;
		
		if self.gravity {
			// Apply Gravity
			self.velocity.y -= gravity_reduce;
			self.velocity.y *= if self.velocity.y < 0.0 {gravity_decell} else {0.9};
		}
		
		// Now do collision checks
		let mut player_box = AxisAlignedBoundingBox::from_position_radius_height(self.position, 0.5, 0.5);
		let mut block_boxes = vec![];
		
		let bpx = self.position.x.floor() as i32;
		let bpy = self.position.y.floor() as i32;
		let bpz = self.position.z.floor() as i32;
		
		for y in (bpy-2)..(bpy+2) {
			for z in (bpz-2)..(bpz+2) {
				for x in (bpx-2)..(bpx+2) {
					let pos = blocks::BlockCoord::new(x, y, z);
					if let Some(block) = chunks.get_block(&pos) {
						// only match NOT AIR for now
						if block.id.raw() != 0 {
							let vec_pos = Vector3::new(x as f32, y as f32, z as f32);
							let block_box = AxisAlignedBoundingBox::from_position_size(vec_pos, Vector3::new(1.0, 1.0, 1.0));
							block_boxes.push(block_box);
						}
					}
				}
			}
		}
		
		for block_box in block_boxes.iter() {
			self.velocity.y = block_box.intersection_y(&player_box, self.velocity.y);
		}
		
		for block_box in block_boxes.iter() {
			self.velocity.x = block_box.intersection_x(&player_box, self.velocity.x);
		}
		
		for block_box in block_boxes.iter() {
			self.velocity.z = block_box.intersection_z(&player_box, self.velocity.z);
		}
		
		// Apply velocity
		self.position += self.velocity;
		
		let air_friction: f32 = 0.975 * delta;
		
		if self.gravity {
			// Apply Friction
			self.velocity.x *= air_friction;
			self.velocity.z *= air_friction;
		} else {
			// Apply Friction
			self.velocity *= air_friction;
		}
	}
}

// This impl-block has to deal with OpenGL shenanigans. Do NOT use for anything but rendering.
impl crate::render::camera::Camera for Freecam {
	fn get_gl_position(&self, interpolation: f32) -> Vector3<f32> {
		self.get_position(interpolation)
	}
	
	fn get_gl_rotation_matrix(&self, _interpolation: f32) -> Matrix4<f32> {
		let pitch = cgmath::Deg(self.rotation.x);
		let yaw   = cgmath::Deg(self.rotation.y);
		let nil = cgmath::Deg(0.0);
		
		let yaw = cgmath::Quaternion::from(cgmath::Euler {
			x: nil, y: yaw, z: nil,
		});
		
		let pitch = cgmath::Quaternion::from(cgmath::Euler {
			x: pitch, y: nil, z: nil,
		});
		
		(pitch * yaw).into()
	}
	
	fn get_gl_projection_matrix(&self, viewport: (i32, i32), _interpolation: f32) -> Matrix4<f32> {
		let (width, height) = viewport;
		
		// Apply velocity to the FoV for speedy-effect
		let field_of_view = if self.fov_vel_effect {
			self.field_of_view + self.velocity.magnitude() * 23.42
		} else {
			self.field_of_view
		};
		
		cgmath::PerspectiveFov {
			fovy: cgmath::Deg(field_of_view).into(),
			aspect: width as f32 / height as f32,
			near: self.min_depth,
			far: self.max_depth,
		}.into()
	}
}

impl std::fmt::Display for Freecam {
	fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(fmt, "Camera({}-mode) [x: {}, y: {}, z: {}, pitch: {}, yaw: {} ] & LastCamera [x: {}, y: {}, z: {}, pitch: {}, yaw: {} ]",
			if self.crane { "crane" } else { "drone" },
			self.position.x,
			self.position.y,
			self.position.z,
			self.rotation.x,
			self.rotation.y,
			self.position_last.x,
			self.position_last.y,
			self.position_last.z,
			self.rotation_last.x,
			self.rotation_last.y,
		)
	}
}

fn clamp(x: f32, min: f32, max: f32) -> f32 {
	if x < min { return min; }
	if x > max { return max; }
	x
}

fn wrap(mut x: f32, r: f32) -> f32 {
	while x < 0.0 {
		x += r;
	}
	while x > r {
		x -= r;
	}
	x
}