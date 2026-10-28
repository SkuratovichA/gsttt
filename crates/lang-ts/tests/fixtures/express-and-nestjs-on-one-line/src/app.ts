import express from 'express';
import { Controller, Get } from '@nestjs/common';

const app = express(); app.get('/b', () => {}); @Controller('x') class C { @Get('a') a() {} }
