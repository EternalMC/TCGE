use crate::blocks as blockdef;
use crate::blocks::BlockCoord;
use crate::client::blocks::ChunkCoord;
use super::super::render::utility::gl_label_object;
use super::static_bakery;
use super::render;
use super::Chunk;
use super::CHUNK_SIZE;
use super::CHUNK_SIZE_MASK;

/// The graphical state of a chunk.
pub enum ChunkMeshState {
	/// Chunk is meshed but empty.
	Empty,
	
	/// Chunk is meshed and full.
	Meshed(render::ChunkMesh),
}


fn get_block(neighbours: &[Option<&Chunk>; 27], pos: &BlockCoord) -> Option<blockdef::BlockState> {
	let cpos = ChunkCoord::new_from_block(pos);
	let csm = CHUNK_SIZE_MASK as isize;
	
	for chunk in neighbours.iter() {
		if let Some(chunk) = chunk {
			if chunk.pos == cpos {
				let cx = pos.x & csm;
				let cy = pos.y & csm;
				let cz = pos.z & csm;
				if let Some(block) = chunk.get_block(cx,cy,cz) {
					return Some(block)
				}
			}
		}
	}
	
	None
}

pub fn mesh(
	blockdef: blockdef::UniverseRef,
	static_bakery: &static_bakery::StaticBlockBakery,
	chunk: &Chunk,
	neighbours: &[Option<&Chunk>; 27]
) -> ChunkMeshState {
	let mut vertices: Vec<ChunkMeshVertex> = vec![];
	let mut quad_buf: Vec<static_bakery::BakedBlockMeshVertex> = vec![];
	quad_buf.reserve(4*6);
	
	let cpos = chunk.pos;
	let cx = cpos.x * (CHUNK_SIZE as isize);
	let cy = cpos.y * (CHUNK_SIZE as isize);
	let cz = cpos.z * (CHUNK_SIZE as isize);
	
	let air = blockdef
		.get_block_by_name_unchecked("air")
		.get_default_state();
	
	let mut context = static_bakery::BakeryContext::new();
	
	let start = crate::util::current_time_nanos();
	
	let mut block_pos = BlockCoord::new(0, 0, 0);
	
	for y in 0..CHUNK_SIZE {
		for z in 0..CHUNK_SIZE {
			for x in 0..CHUNK_SIZE {
				let x = x as isize;
				let y = y as isize;
				let z = z as isize;
				let block = unsafe {chunk.get_block_unchecked(x, y, z)};
				
				if block == air {
					continue;
				}
				
				let cbx = x + cx;
				let cby = y + cy;
				let cbz = z + cz;
				block_pos.set(cbx, cby, cbz);
				
				let cbp = vertices.len();
				
				context.set_occlusion(
					get_block(neighbours, &block_pos.right   (1)).unwrap_or(air) != air,
					get_block(neighbours, &block_pos.up      (1)).unwrap_or(air) != air,
					get_block(neighbours, &block_pos.backward(1)).unwrap_or(air) != air,
					get_block(neighbours, &block_pos.left    (1)).unwrap_or(air) != air,
					get_block(neighbours, &block_pos.down    (1)).unwrap_or(air) != air,
					get_block(neighbours, &block_pos.forward (1)).unwrap_or(air) != air,
					true
				);
				
				static_bakery.render_block(&context, &block, &mut quad_buf);
				
				for quad in quad_buf.chunks_exact(4) {
					vertices.reserve(6);
					vertices.push(quad[0].into()); // a
					vertices.push(quad[1].into()); // b
					vertices.push(quad[3].into()); // d
					vertices.push(quad[1].into()); // b
					vertices.push(quad[2].into()); // c
					vertices.push(quad[3].into()); // d
				}
				
				quad_buf.clear();
				
				for vertex in &mut vertices[cbp..] {
					vertex.x += cbx as f32;
					vertex.y += cby as f32;
					vertex.z += cbz as f32;
				}
			}
		}
	}
	
	let end = crate::util::current_time_nanos();
	debug!("Took {}ns to mesh chunk {}.", end - start, chunk.pos);
	
	return upload(chunk, &vertices);
}

fn upload(chunk: &Chunk, mesh_data: &Vec<ChunkMeshVertex>) -> ChunkMeshState {
	// Don't upload empty meshes.
	if mesh_data.len() == 0 {
		return ChunkMeshState::Empty
	}
	
	let vertex_count = mesh_data.len();
	
	let mut vbo: gl::types::GLuint = 0;
	unsafe {
		gl::GenBuffers(1, &mut vbo);
		gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
		gl::BufferData(
			gl::ARRAY_BUFFER,
			(vertex_count * std::mem::size_of::<ChunkMeshVertex>()) as gl::types::GLsizeiptr,
			mesh_data.as_ptr() as *const gl::types::GLvoid,
			gl::STATIC_DRAW
		);
		gl::BindBuffer(gl::ARRAY_BUFFER, 0);
	}
	
	let mut vao: gl::types::GLuint = 0;
	unsafe {
		gl::GenVertexArrays(1, &mut vao);
		gl::BindVertexArray(vao);
		gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
		
		gl::EnableVertexAttribArray(0);
		gl::VertexAttribPointer(
			0, // attribute location
			3, // sub-element count
			gl::FLOAT, // sub-element type
			gl::FALSE, // sub-element normalization
			(5 * std::mem::size_of::<f32>()) as gl::types::GLsizei,
			(0 * std::mem::size_of::<f32>()) as *const gl::types::GLvoid
		);
		
		gl::EnableVertexAttribArray(1);
		gl::VertexAttribPointer(
			1, // attribute location
			2, // sub-element count
			gl::FLOAT, // sub-element type
			gl::FALSE, // sub-element normalization
			(5 * std::mem::size_of::<f32>()) as gl::types::GLsizei,
			(3 * std::mem::size_of::<f32>()) as *const gl::types::GLvoid
		);
		
		gl::BindBuffer(gl::ARRAY_BUFFER, 0);
		gl::BindVertexArray(0);
	}
	
	let label = format!("Chunk({}, {}, {})", chunk.pos.x, chunk.pos.y, chunk.pos.z);
	
	gl_label_object(
		gl::VERTEX_ARRAY, vao,
		&format!("{} Descriptor", label)
	);
	
	gl_label_object(
		gl::BUFFER, vbo,
		&format!("{} Geometry", label)
	);
	
	return ChunkMeshState::Meshed(render::ChunkMesh::new(
		vao,
		vbo,
		vertex_count as i32
	))
}

#[derive(Copy, Clone, Debug)]
#[repr(C, packed)]
struct ChunkMeshVertex {
	// Geometry
	pub x: f32,
	pub y: f32,
	pub z: f32,
	
	// Texture
	pub u: f32,
	pub v: f32,
}

impl ChunkMeshVertex {
	pub fn new(x: f32, y: f32, z: f32, u: f32, v: f32) -> Self {
		Self {
			x, y, z, u, v
		}
	}
}

impl From<(f32, f32, f32, f32, f32)> for ChunkMeshVertex {
	fn from(other: (f32, f32, f32, f32, f32)) -> Self {
		Self::new(other.0, other.1, other.2, other.3, other.4)
	}
}

impl From<static_bakery::BakedBlockMeshVertex> for ChunkMeshVertex {
	fn from(other: static_bakery::BakedBlockMeshVertex) -> Self {
		Self::new(other.x, other.y, other.z, other.u, other.v)
	}
}
