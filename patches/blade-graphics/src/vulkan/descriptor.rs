use ash::vk;

//TODO: replace by an abstraction in `gpu-descriptor`
// https://github.com/zakarumych/gpu-descriptor/issues/42
const COUNT_BASE: u32 = 16;
/// Upper bound on descriptor sets in a single sub-pool. The historical
/// `COUNT_BASE.pow(iter)` schedule grew to 4096-set pools, which Intel's
/// Vulkan driver mapped at ~50 MiB each; two command buffers pushed RSS past
/// 100 MiB. Capping the pool size keeps every allocation modest.
const MAX_SETS_PER_POOL: u32 = 256;
/// Growth is a bounded doubling from [`COUNT_BASE`], so the k-th extra pool
/// is `COUNT_BASE << min(k, MAX_GROWTH_EXP)`, never `COUNT_BASE.pow(k)`.
const MAX_GROWTH_EXP: u32 = (MAX_SETS_PER_POOL / COUNT_BASE).ilog2();
/// Hard ceiling on the number of sub-pools created inside one command buffer,
/// so a pathological frame cannot grow memory without bound.
const MAX_SUB_POOLS: usize = 64;

#[derive(Debug)]
pub struct DescriptorPool {
    sub_pools: Vec<vk::DescriptorPool>,
    growth_iter: usize,
}

impl super::Device {
    fn create_descriptor_sub_pool(&self, max_sets: u32) -> vk::DescriptorPool {
        log::info!("Creating a descriptor pool for at most {} sets", max_sets);
        let mut descriptor_sizes = vec![
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::INLINE_UNIFORM_BLOCK_EXT,
                descriptor_count: max_sets * crate::limits::PLAIN_DATA_SIZE,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: max_sets,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::SAMPLED_IMAGE,
                descriptor_count: 2 * max_sets,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::SAMPLER,
                descriptor_count: max_sets,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_IMAGE,
                descriptor_count: max_sets,
            },
        ];
        if self.ray_tracing.is_some() {
            descriptor_sizes.push(vk::DescriptorPoolSize {
                ty: vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
                descriptor_count: max_sets,
            });
        }

        let mut inline_uniform_block_info = vk::DescriptorPoolInlineUniformBlockCreateInfoEXT {
            max_inline_uniform_block_bindings: max_sets,
            ..Default::default()
        };

        let descriptor_pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(max_sets)
            .flags(self.workarounds.extra_descriptor_pool_create_flags)
            .pool_sizes(&descriptor_sizes)
            .push_next(&mut inline_uniform_block_info);

        unsafe {
            self.core
                .create_descriptor_pool(&descriptor_pool_info, None)
                .unwrap()
        }
    }

    pub(super) fn create_descriptor_pool(&self) -> DescriptorPool {
        let vk_pool = self.create_descriptor_sub_pool(COUNT_BASE);
        DescriptorPool {
            sub_pools: vec![vk_pool],
            growth_iter: 0,
        }
    }

    pub(super) fn destroy_descriptor_pool(&self, pool: &mut DescriptorPool) {
        for sub_pool in pool.sub_pools.drain(..) {
            unsafe { self.core.destroy_descriptor_pool(sub_pool, None) };
        }
    }

    pub(super) fn allocate_descriptor_set(
        &self,
        pool: &mut DescriptorPool,
        layout: &super::DescriptorSetLayout,
    ) -> vk::DescriptorSet {
        let descriptor_set_layouts = [layout.raw];

        loop {
            let descriptor_set_info = vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(pool.sub_pools[0])
                .set_layouts(&descriptor_set_layouts);
            match unsafe { self.core.allocate_descriptor_sets(&descriptor_set_info) } {
                Ok(vk_sets) => return vk_sets[0],
                Err(vk::Result::ERROR_OUT_OF_POOL_MEMORY)
                | Err(vk::Result::ERROR_FRAGMENTED_POOL) => {}
                Err(other) => panic!("Unexpected descriptor allocation error: {:?}", other),
            };

            let exp = (pool.growth_iter + 1).min(MAX_GROWTH_EXP as usize);
            let next_max_sets =
                ((COUNT_BASE as usize) << exp).min(MAX_SETS_PER_POOL as usize) as u32;
            if pool.sub_pools.len() >= MAX_SUB_POOLS {
                log::warn!(
                    "descriptor pool has grown to {} sub-pools ({MAX_SUB_POOLS} pool budget); adding a {next_max_sets}-set pool",
                    pool.sub_pools.len()
                );
            }
            pool.growth_iter += 1;
            let vk_pool = self.create_descriptor_sub_pool(next_max_sets);
            pool.sub_pools.insert(0, vk_pool);
        }
    }

    pub(super) fn reset_descriptor_pool(&self, pool: &mut DescriptorPool) {
        for vk_pool in pool.sub_pools.drain(..) {
            unsafe {
                self.core.destroy_descriptor_pool(vk_pool, None);
            }
        }

        // Rebuild from a single base-size pool and re-arm the growth schedule.
        // The previous implementation kept `sub_pools[0]`, which after growth
        // was the largest pool (up to 4096 sets on the old exponential
        // schedule). Retaining it kept RSS high across frames even after
        // `vkResetDescriptorPool`, because the pool kept its capacity.
        let vk_pool = self.create_descriptor_sub_pool(COUNT_BASE);
        pool.sub_pools.push(vk_pool);
        pool.growth_iter = 0;
        unsafe {
            self.core
                .reset_descriptor_pool(vk_pool, vk::DescriptorPoolResetFlags::empty())
                .unwrap();
        }
    }
}
