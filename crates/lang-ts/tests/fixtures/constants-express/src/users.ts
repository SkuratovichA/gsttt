import { Router } from 'express';
import { PATHS } from './paths';

const router = Router();
router.get(PATHS.USERS, (req, res) => res.json([]));

export default router;
