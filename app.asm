.section .text.main,"ax",%progbits
	.globl	main
	.p2align	1
	.type	main,%function
	.code	16
	.thumb_func
main:
		// /home/dom/Projects/UNO-R4-rust/src/main.rs:11
		#[entry]
	.fnstart
	.cfi_sections .debug_frame
	.cfi_startproc
	.save	{r7, lr}
	push {r7, lr}
	.cfi_def_cfa_offset 8
	.cfi_offset lr, -4
	.cfi_offset r7, -8
	.setfp	r7, sp
	mov r7, sp
	.cfi_def_cfa_register r7
	bl uno_r4_rust::__cortex_m_rt_main
