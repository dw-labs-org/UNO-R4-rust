.section .text.main,"ax",%progbits
	.globl	main
	.p2align	1
	.type	main,%function
	.code	16
	.thumb_func
main:
	.fnstart
	.cfi_startproc
	movw r0, :lower16:DEVICE_PERIPHERALS
	movs r1, #1
	movt r0, :upper16:DEVICE_PERIPHERALS
	movs r2, #0
	strb r1, [r0]
	movs r0, #32
	movt r0, #16388
	mov.w r1, #2048
	strh r1, [r0, #2]
.LBB4_1:
	strh r1, [r0]
	strh r2, [r0]
	strh r1, [r0]
	strh r2, [r0]
	strh r1, [r0]
	strh r2, [r0]
	strh r1, [r0]
	strh r2, [r0]
	b .LBB4_1
