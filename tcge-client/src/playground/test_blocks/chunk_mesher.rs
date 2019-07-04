use super::*;
use crate::render;
use half::f16;

/// The graphical state of a chunk.
pub enum ChunkMeshState {
	/// Chunk is meshed but empty.
	Empty,
	
	/// Chunk is meshed and full.
	Meshed(ChunkMesh),
}

/// The graphical representation of a chunk.
/// Really just a bag of OpenGL Object-Handles.
pub struct ChunkMesh {
	gl: gl::Gl,
	descriptor: gl::types::GLuint,
	vertices: render::BufferObject,
	count: i32,
}

impl ChunkMesh {
	pub fn new(gl: &gl::Gl, descriptor: gl::types::GLuint, vertices: render::BufferObject, count: i32) -> Self {
		Self {
			gl: gl.clone(),
			descriptor,
			vertices,
			count
		}
	}
	
	pub fn draw(&self) {
		unsafe {
			self.gl.BindVertexArray(self.descriptor);
			self.gl.DrawElements(
				gl::TRIANGLES,
				self.count,
				gl::UNSIGNED_SHORT,
				0 as *const gl::types::GLvoid
			);
		}
	}
}

impl Drop for ChunkMesh {
	fn drop(&mut self) {
		unsafe {
			let tmp = [self.vertices.id];
			self.gl.DeleteBuffers(1, tmp.as_ptr());
			
			let tmp = [self.descriptor];
			self.gl.DeleteVertexArrays(1, tmp.as_ptr());
		}
	}
}

#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
struct ChunkMeshVertex {
	// Geometry
	pub x: half::f16,
	pub y: half::f16,
	pub z: half::f16,
	
	// Texture
	pub u: half::f16,
	pub v: half::f16,
	
	// AO
	pub ao: half::f16,
}

impl ChunkMeshVertex {
	pub fn new(x: f16, y: f16, z: f16, u: f16, v: f16, ao: f16) -> Self {
		Self {
			x, y, z, u, v, ao
		}
	}
	
	pub fn new_from(other: &BakedBlockMeshVertex, ao: f32, offset: &(f32, f32, f32)) -> Self{
		Self {
			x: f16::from_f32(other.x + offset.0),
			y: f16::from_f32(other.y + offset.1),
			z: f16::from_f32(other.z + offset.2),
			u: f16::from_f32(other.u),
			v: f16::from_f32(other.v),
			ao: f16::from_f32(ao)
		}
	}
}

pub struct MesherThreadState {
	vertices: Vec<ChunkMeshVertex>,
	quad_buf: Vec<BakedBlockMeshVertex>
}

impl MesherThreadState {
	pub fn new() -> MesherThreadState {
		MesherThreadState {
			vertices: vec![],
			quad_buf: vec![],
		}
	}
	
	pub fn reset(&mut self) {
		self.vertices.clear();
		self.quad_buf.clear();
	}
}


pub fn mesh_chunk(
	gl: &gl::Gl,
	qindex: &render::BufferObject,
	mesher: &mut MesherThreadState,
	blocks: BlocksRef,
	static_bakery: &StaticBlockBakery,
	chunk: &Chunk,
	block_data: &ChunkWithEdge
) -> ChunkMeshState {
	let start = common::current_time_nanos_precise();
	
	let premesh = start;
	
	// --- Reset state of the mesher, clearing the buffers.
	mesher.reset();
	let vertices = &mut mesher.vertices;
	
	let air = blocks
		.get_block_by_name_unchecked("air")
		.get_default_state();
	
	let (cx, cy, cz) = chunk.pos.to_block_coord_tuple();
	
	// --- Local function for fetching blocks quickly...
	let get_block = |
		local_x: BlockDim,
		local_y: BlockDim,
		local_z: BlockDim,
	| {
		// Local minima is 0, maxima is +17: The standard range of 0..CHUNK_SIZE+2
		(unsafe {
			block_data
				.get_unchecked((local_y+1) as usize)
				.get_unchecked((local_z+1) as usize)
				.get_unchecked((local_x+1) as usize).clone()
		})
	};
	
	let mut context = BakeryContext::new();
	
	let premesh = common::current_time_nanos_precise() - premesh;
	let mut starts = (start, start);
	let mut length = (0, 0);
	
	let mut non_empty = 0;
	
	for y in 0..CHUNK_SIZE {
		for z in 0..CHUNK_SIZE {
			for x in 0..CHUNK_SIZE {
				// starts.0 = common::current_time_nanos_precise();
				
				let x = x as BlockDim;
				let y = y as BlockDim;
				let z = z as BlockDim;
				
				let block = get_block(x, y, z);
				
				if block == air {
					// length.0 += common::current_time_nanos_precise() - starts.0;
					continue;
				}
				
				non_empty += 1;
				
				context.set_occlusion(
					get_block(x+1, y, z) != air,
					get_block(x, y+1, z) != air,
					get_block(x, y, z+1) != air,
					get_block(x-1, y, z) != air,
					get_block(x, y-1, z) != air,
					get_block(x, y, z-1) != air,
					true
				);
				
				// length.0 += common::current_time_nanos_precise() - starts.0;
				
				// starts.1 = common::current_time_nanos_precise();
				let cbx = x + cx;
				let cby = y + cy;
				let cbz = z + cz;
				let offset = (cbx as f32, cby as f32, cbz as f32);
				
				static_bakery.render_block(&context, &block, &mut |face| {
					vertices.push(ChunkMeshVertex::new_from(&face.a, 0.0, &offset));
					vertices.push(ChunkMeshVertex::new_from(&face.b, 0.0, &offset));
					vertices.push(ChunkMeshVertex::new_from(&face.c, 0.0, &offset));
					vertices.push(ChunkMeshVertex::new_from(&face.d, 0.0, &offset));
				});
				// length.1 += common::current_time_nanos_precise() - starts.1;
			}
		}
	}
	
	let duration = (common::current_time_nanos_precise() - start) as f64;
	if duration > 100.0 {
		trace!("Took {:.0}ns ({:.0}% pre, {:.0}% occ, {:.0}% cpy) to mesh chunk {} ({} solids)",
			duration,
			premesh as f64 / duration * 100.0,
			length.0 as f64 / duration * 100.0,
			length.1 as f64 / duration * 100.0,
			chunk.pos,
			non_empty
		);
	}
	
	return upload(gl, chunk, &vertices, &qindex);
}

fn upload(gl: &gl::Gl, chunk: &Chunk, mesh_data: &Vec<ChunkMeshVertex>, qindex: &render::BufferObject) -> ChunkMeshState {
	// Don't upload empty meshes.
	if mesh_data.len() == 0 {
		return ChunkMeshState::Empty
	}
	
	let vertex_count = mesh_data.len() / 4 * 6;
	
	let vbo = render::BufferObject::buffer_data(gl, gl::ARRAY_BUFFER, gl::STATIC_DRAW, mesh_data);
	
	let mut vao: gl::types::GLuint = 0;
	unsafe {
		gl.GenVertexArrays(1, &mut vao);
		gl.BindVertexArray(vao);
		gl.BindBuffer(gl::ARRAY_BUFFER, vbo.id);
		
		// Bind the index buffer
		gl.BindBuffer(qindex.target, qindex.id);
		
		let stride = (6 * std::mem::size_of::<f16>()) as gl::types::GLsizei;
		
		gl.EnableVertexAttribArray(0);
		gl.VertexAttribPointer(
			0, // attribute location
			3, // sub-element count
			gl::HALF_FLOAT, // sub-element type
			gl::FALSE, // sub-element normalization
			stride,
			(0 * std::mem::size_of::<f16>()) as *const gl::types::GLvoid
		);
		
		gl.EnableVertexAttribArray(1);
		gl.VertexAttribPointer(
			1, // attribute location
			2, // sub-element count
			gl::HALF_FLOAT, // sub-element type
			gl::FALSE, // sub-element normalization
			stride,
			(3 * std::mem::size_of::<f16>()) as *const gl::types::GLvoid
		);
		
		gl.EnableVertexAttribArray(2);
		gl.VertexAttribPointer(
			2, // attribute location
			1, // sub-element count
			gl::HALF_FLOAT, // sub-element type
			gl::FALSE, // sub-element normalization
			stride,
			(5 * std::mem::size_of::<f16>()) as *const gl::types::GLvoid
		);
		
		gl.BindVertexArray(0);
	}
	
	let label = format!("Chunk({}, {}, {})", chunk.pos.x, chunk.pos.y, chunk.pos.z);
	
	gl.label_object(
		gl::VERTEX_ARRAY, vao,
		&format!("{} Descriptor", label)
	);
	
	gl.label_object(
		gl::BUFFER, vbo.id,
		&format!("{} Geometry", label)
	);
	
	ChunkMeshState::Meshed(ChunkMesh::new(
		gl,
		vao,
		vbo,
		vertex_count as i32
	))
}

fn lerp_trilinear(x: f32, y: f32, z: f32, corners: &[f32; 8]) -> f32 {
	(1.0 - x) * (1.0 - y) * (1.0 - z) * corners[0] +
		x * (1.0 - y) * (1.0 - z) * corners[1] +
		(1.0 - x) * y * (1.0 - z) * corners[2] +
		x * y * (1.0 - z) * corners[3] +
		(1.0 - x) * (1.0 - y) * z * corners[4] +
		x * (1.0 - y) * z * corners[5] +
		(1.0 - x) * y * z * corners[6] +
		x * y * z * corners[7]
}
