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
	neighbours: &[Option<&Chunk>; 27]
) -> ChunkMeshState {
	let start = current_time_nanos();
	
	// --- Reset state of the mesher, clearing the buffers.
	mesher.reset();
	let vertices = &mut mesher.vertices;
	
	let air = blocks
		.get_block_by_name_unchecked("air")
		.get_default_state();
	
	let (cx, cy, cz) = chunk.pos.to_block_coord();
	
	// --- Local function for fetching blocks quickly...
	let get_block = |
		offset: &BlockCoord,
	| {
		if chunk.pos.contains_block(offset) {
			return Some(unsafe {
				chunk.get_block_unchecked(offset.x, offset.y, offset.z)
			})
		}
		
		let o_cpos = ChunkCoord::new_from_block(offset);
		for o_chunk in neighbours.iter() {
			if let Some(o_chunk) = o_chunk {
				if o_chunk.pos == o_cpos {
					return Some(unsafe {
						o_chunk.get_block_unchecked(offset.x, offset.y, offset.z)
					})
				}
			}
		}
		
		None
	};
	
	let mut block_pos = BlockCoord::new(0, 0, 0);
	let mut context = BakeryContext::new();
	
	for y in 0..CHUNK_SIZE {
		for z in 0..CHUNK_SIZE {
			for x in 0..CHUNK_SIZE {
				let x = x as BlockDim;
				let y = y as BlockDim;
				let z = z as BlockDim;
				let block = unsafe {chunk.get_block_unchecked(x, y, z)};
				
				if block == air {
					continue;
				}
				
				let cbx = x + cx;
				let cby = y + cy;
				let cbz = z + cz;
				block_pos.set(cbx, cby, cbz);
				
				let offset = (cbx as f32, cby as f32, cbz as f32);
				
				context.set_occlusion(
					get_block(&block_pos.right   (1)).unwrap_or(air) != air,
					get_block(&block_pos.up      (1)).unwrap_or(air) != air,
					get_block(&block_pos.backward(1)).unwrap_or(air) != air,
					get_block(&block_pos.left    (1)).unwrap_or(air) != air,
					get_block(&block_pos.down    (1)).unwrap_or(air) != air,
					get_block(&block_pos.forward (1)).unwrap_or(air) != air,
					true
				);
				
				static_bakery.render_block(&context, &block, &mut |face| {
					vertices.push(ChunkMeshVertex::new_from(&face.a, 0.0, &offset));
					vertices.push(ChunkMeshVertex::new_from(&face.b, 0.0, &offset));
					vertices.push(ChunkMeshVertex::new_from(&face.c, 0.0, &offset));
					vertices.push(ChunkMeshVertex::new_from(&face.d, 0.0, &offset));
				});
				
			}
		}
	}
	
	let end = current_time_nanos();
	let duration = end - start;
	if duration > 100 {
		debug!("Took {}ns to mesh chunk {}.", duration, chunk.pos);
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
